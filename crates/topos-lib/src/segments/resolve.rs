use crate::{
    data::chapter_verses::ChapterVerses,
    segments::{
        grammar::{Following, Number, NumberKind, SegmentNode},
        passage::Segments,
        segment::Segment,
        verse_bounds::VerseBounds,
    },
};

/**
Turns parsed [`SegmentNode`]s into [`Segments`], deciding what each number means

- A bare number is a chapter until a verse has been seen (`John 1, 3` is chapters 1 and 3)
- After a verse, a bare number is a verse in the same chapter (`John 3:16, 18`)
- A `;` starts chapters again (`John 3:16; 4` is chapter 4)
- Older references put a verse after a Roman numeral chapter with a comma (`Matth. x, 8` is 10:8)
- Roman numerals are only accepted where a chapter goes, so `John 3:16, I think` stops at `3:16`
- With versification data:
  - single-chapter books treat bare numbers as verses (`Jude 5` is `Jude 1:5`)
  - resolution stops at the first segment that does not exist (`Mark 25`)
*/
#[derive(Clone, Copy, Debug, Default)]
pub struct Resolver<'a> {
    versification: Option<&'a ChapterVerses>,
}

/// The resolved segments, and what part of the input they came from
#[derive(Clone, Debug)]
pub struct Resolved {
    pub segments: Segments,
    /// How many nodes were used (the last one possibly shortened)
    pub used: usize,
    /// Byte offset where the accepted text ends (0 when nothing was accepted)
    pub end: usize,
}

impl<'a> Resolver<'a> {
    /// Resolve with the chapter and verse counts of the matched book
    pub fn for_book(versification: Option<&'a ChapterVerses>) -> Self {
        Self { versification }
    }

    /**
    Resolves nodes until one cannot be resolved
    - A node that does not exist is retried without a trailing part that starts with `.`, so in
      `Matthew 6, 24.\n143` the `.143` is dropped and `24` is kept (but `John 3:37` is rejected)
    - Nothing after a shortened node is used
    */
    pub fn resolve(&self, nodes: &[SegmentNode]) -> Resolved {
        let mut segments = Segments::new();
        let mut prev_node: Option<SegmentNode> = None;
        let mut used = 0;
        let mut end = 0;
        for node in nodes {
            // Only a `.` can be dropped, since it may end a sentence rather than start a verse
            let parts: Vec<_> = node.parts().collect();
            let keeps = (0..parts.len())
                .rev()
                .filter(|&keep| parts[keep].delimiter.actual == '.');
            let accepted = std::iter::once(parts.len()).chain(keeps).find_map(|keep| {
                let node = node.truncated(keep);
                let segment = self.resolve_node(&node, prev_node.as_ref(), segments.last())?;
                let segment = self.apply_following(&node, segment);
                self.exists(&segment).then_some((node, segment))
            });
            let Some((accepted, segment)) = accepted else {
                break;
            };
            // In `x, 8` the comma separates chapter and verse, so `10:8` replaces `10`
            if is_old_style(&accepted, prev_node.as_ref()) {
                segments.pop();
            }
            segments.push(segment);
            used += 1;
            end = accepted.complete_end();
            if accepted != *node {
                break;
            }
            prev_node = Some(accepted);
        }
        Resolved {
            segments,
            used,
            end,
        }
    }

    /**
    The chapter that the next segment's first number is a verse in, or [`None`] if it is a
    chapter, given the separator before it and the segments so far: the same rules as
    resolving, for completing a segment that is still being typed
    - `John 3:16, 1` is a verse (in chapter 3), `John 3:16; 1` and `John 3, 1` are chapters
    */
    pub fn next_verse_chapter(
        &self,
        separator: Option<char>,
        prev_node: Option<&SegmentNode>,
        prev: Option<&Segment>,
    ) -> Option<u8> {
        if self.single_chapter() {
            return Some(1);
        }
        let prev = prev?;
        if separator == Some(';') {
            return None;
        }
        // `Matth. x, 8`: after a Roman numeral chapter, a comma starts its verse
        let old_style = separator == Some(',')
            && prev_node
                .is_some_and(|p| p.start.kind == NumberKind::Roman && p.parts().next().is_none());
        if prev.ending_verse().is_none() && !old_style {
            return None;
        }
        Some(prev.ending_chapter())
    }

    fn single_chapter(&self) -> bool {
        self.versification
            .is_some_and(ChapterVerses::has_one_chapter)
    }

    /// The chapter that a bare starting number is a verse in, or [`None`] if it is a chapter
    fn verse_context(
        &self,
        node: &SegmentNode,
        prev_node: Option<&SegmentNode>,
        prev: Option<&Segment>,
    ) -> Option<u8> {
        if self.single_chapter() {
            return Some(1);
        }
        let prev = prev?;
        if node.separator.is_some_and(|s| s.actual == ';') {
            return None;
        }
        if prev.ending_verse().is_none() && !is_old_style(node, prev_node) {
            return None;
        }
        Some(prev.ending_chapter())
    }

    fn resolve_node(
        &self,
        node: &SegmentNode,
        prev_node: Option<&SegmentNode>,
        prev: Option<&Segment>,
    ) -> Option<Segment> {
        let start = node.start.value;
        let verse_context = self.verse_context(node, prev_node, prev);
        let start_verse = node.start_verse.and_then(|p| p.number);
        let end = node.end.and_then(|p| p.number);
        let end_verse = node.end_verse.and_then(|p| p.number);

        // Roman numerals only where a chapter goes
        let start_is_chapter = verse_context.is_none() || start_verse.is_some();
        let end_is_chapter =
            end_verse.is_some() || (verse_context.is_none() && start_verse.is_none());
        let roman = |n: Option<Number>| n.is_some_and(|n| n.kind == NumberKind::Roman);
        if (!start_is_chapter && roman(Some(node.start)))
            || roman(start_verse)
            || roman(end_verse)
            || (!end_is_chapter && roman(end))
        {
            return None;
        }

        let value = |n: Option<Number>| n.map(|n| n.value);
        Some(match (value(start_verse), value(end), value(end_verse)) {
            // `1:2-3:4`
            (Some(sv), Some(ec), Some(ev)) => Segment::chapter_range(start, sv, ec, ev),
            // `1:2-3`
            (Some(sv), Some(ev), None) => Segment::chapter_verse_range(start, sv, ev),
            // `1:2`
            (Some(sv), None, _) => Segment::chapter_verse(start, sv),
            (None, Some(ec), Some(ev)) => match verse_context {
                // `5:7, 12-6:6` (verse 12 of chapter 5 to 6:6)
                Some(chapter) => Segment::chapter_range(chapter, start, ec, ev),
                // `1-2:3`
                None => Segment::chapter_range(start, 1, ec, ev),
            },
            (None, Some(end), None) => match verse_context {
                // `3:1, 4-5`
                Some(chapter) => Segment::chapter_verse_range(chapter, start, end),
                // `1-25`
                None => Segment::full_chapter_range(start, end),
            },
            (None, None, _) => match verse_context {
                // `3:1, 4`
                Some(chapter) => Segment::chapter_verse(chapter, start),
                // `1`
                None => Segment::full_chapter(start),
            },
        })
    }

    /**
    Extends a segment ending in `f` or `ff`
    - `3:16f` is `3:16-17`, and `3:16ff` is `3:16-36` (the end of the chapter)
    - Without versification, `ff` cannot be resolved and is left as written
    */
    fn apply_following(&self, node: &SegmentNode, segment: Segment) -> Segment {
        let (Some(following), Some(end_verse)) =
            (node.last_number().following, segment.ending_verse())
        else {
            return segment;
        };
        let chapter = segment.ending_chapter();
        let last = self.versification.and_then(|v| v.get_last_verse(chapter));
        let end = match (following, last) {
            (Following::Next, Some(last)) => end_verse.saturating_add(1).min(last),
            (Following::Next, None) => end_verse.saturating_add(1),
            (Following::Rest, Some(last)) => last,
            (Following::Rest, None) => return segment,
        };
        Segment::chapter_range(
            segment.starting_chapter(),
            segment.starting_verse(),
            chapter,
            end,
        )
    }

    /// Whether every chapter and verse exists (always true without versification data)
    fn exists(&self, segment: &Segment) -> bool {
        let Some(versification) = self.versification else {
            return true;
        };
        let verse_exists = |chapter: u8, verse: u8| {
            verse >= 1
                && versification
                    .get_last_verse(chapter)
                    .is_some_and(|last| verse <= last)
        };
        let start_exists = verse_exists(segment.starting_chapter(), segment.starting_verse());
        let end_exists = match segment.ending_verse() {
            Some(verse) => verse_exists(segment.ending_chapter(), verse),
            None => verse_exists(segment.ending_chapter(), 1),
        };
        let ordered = (segment.starting_chapter(), segment.starting_verse())
            <= (
                segment.ending_chapter(),
                segment.ending_verse().unwrap_or(u8::MAX),
            );
        start_exists && end_exists && ordered
    }
}

/// `Matth. x, 8`: a decimal number after a bare Roman numeral chapter and a comma
fn is_old_style(node: &SegmentNode, prev_node: Option<&SegmentNode>) -> bool {
    let after_roman_chapter =
        prev_node.is_some_and(|p| p.start.kind == NumberKind::Roman && p.parts().next().is_none());
    after_roman_chapter
        && node.start.kind == NumberKind::Decimal
        && node.start_verse.is_none()
        && node.separator.is_some_and(|s| s.actual == ',')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::{BibleData, books::BookId},
        segments::grammar::SegmentList,
    };

    fn resolve(book: Option<u8>, input: &str) -> String {
        let data = BibleData::default();
        let versification = book.and_then(|id| {
            data.chapter_verses()
                .get_chapter_verses(&BookId(id))
                .cloned()
        });
        let list = SegmentList::parse(input);
        let resolved = Resolver::for_book(versification.as_ref()).resolve(&list.nodes);
        resolved
            .segments
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[test]
    fn chapters_until_a_verse() {
        assert_eq!(resolve(None, "1, 3"), "1, 3");
        assert_eq!(resolve(None, "1-2, 4-5"), "1-2, 4-5");
        assert_eq!(resolve(None, "1, 3:16, 18"), "1, 3:16, 3:18");
        assert_eq!(resolve(None, "3:16, 18-20"), "3:16, 3:18-20");
    }

    #[test]
    fn semicolon_starts_chapters() {
        assert_eq!(resolve(None, "3:16; 4"), "3:16, 4");
        assert_eq!(resolve(None, "3:16; 4:1"), "3:16, 4:1");
        assert_eq!(resolve(None, "3:16, 4"), "3:16, 3:4");
    }

    #[test]
    fn roman_numerals() {
        assert_eq!(resolve(None, "x, 8"), "10:8");
        assert_eq!(resolve(None, "x, 8-10"), "10:8-10");
        assert_eq!(resolve(None, "x:8"), "10:8");
        assert_eq!(resolve(None, "iii-v"), "3-5");
        // a Roman numeral where a verse goes ends the reference
        assert_eq!(resolve(None, "3:16, i"), "3:16");
        assert_eq!(resolve(None, "3:iv"), "");
    }

    #[test]
    fn following_verses() {
        // John 3 has 36 verses
        assert_eq!(resolve(Some(43), "3:16f"), "3:16-17");
        assert_eq!(resolve(Some(43), "3:16ff"), "3:16-36");
        assert_eq!(resolve(Some(43), "3:36f"), "3:36");
        assert_eq!(resolve(Some(43), "3:16-18 ff"), "3:16-36");
        assert_eq!(resolve(Some(43), "3:16, 20ff"), "3:16, 3:20-36");
        // Jude has 25 verses
        assert_eq!(resolve(Some(65), "5ff"), "1:5-25");
        // A chapter is unaffected, and `ff` needs versification
        assert_eq!(resolve(Some(43), "3ff"), "3");
        assert_eq!(resolve(None, "3:16ff"), "3:16");
    }

    #[test]
    fn single_chapter_books() {
        // Jude
        assert_eq!(resolve(Some(65), "5"), "1:5");
        assert_eq!(resolve(Some(65), "5-8"), "1:5-8");
        assert_eq!(resolve(Some(65), "5, 7"), "1:5, 1:7");
        assert_eq!(resolve(Some(65), "1:5"), "1:5");
    }

    #[test]
    fn shortens_nodes_that_do_not_exist() {
        // Matthew 24 has 51 verses
        assert_eq!(resolve(Some(40), "6, 24.143"), "6, 24");
        assert_eq!(resolve(Some(43), "3.16-99"), "3");
        assert_eq!(resolve(Some(43), "3:16-99"), "");
        assert_eq!(resolve(Some(43), "3:16, 4.99"), "3:16, 3:4");
    }

    #[test]
    fn validates_against_versification() {
        // Mark has 16 chapters
        assert_eq!(resolve(Some(41), "25"), "");
        assert_eq!(resolve(Some(41), "16, 25"), "16");
        // John 3 has 36 verses
        assert_eq!(resolve(Some(43), "3:36"), "3:36");
        assert_eq!(resolve(Some(43), "3:37"), "");
        assert_eq!(resolve(Some(43), "3:18-16"), "");
        assert_eq!(resolve(Some(43), "0"), "");
        // Without data, anything goes
        assert_eq!(resolve(None, "25"), "25");
    }
}

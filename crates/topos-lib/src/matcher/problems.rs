use std::ops::Range;

use crate::{
    matcher::{BibleMatcher, text::SearchText},
    segments::{
        grammar::SegmentList,
        resolve::{Resolver, TOO_LARGE},
        verse_bounds::VerseBounds,
    },
};

/// A reference that is written like one but does not exist (`John 3:99`)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// Byte range of the segment that does not exist
    pub bytes: Range<usize>,
    pub message: String,
    /// What does exist: `John 3 has 36 verses`, or `John has 21 chapters`
    pub detail: Option<String>,
}

impl BibleMatcher {
    /**
    Segments that parse but do not exist in their book, like `John 3:99` or `John 3:16, 4:99`
    - Only segments with an explicit `chapter:verse` are reported, so prose like the date in
      `Mar 25, 2025` is left alone
    */
    pub fn problems(&self, text: &str) -> Vec<Problem> {
        let data = self.data();
        let text = SearchText::new(text);
        let starts: Vec<_> = data.books().candidates(text.as_str()).collect();
        let mut problems = vec![];
        for (idx, cur) in starts.iter().enumerate() {
            let Some(book) = data.books().search(cur.as_str()) else {
                continue;
            };
            let end = starts
                .get(idx + 1)
                .map_or(text.as_str().len(), |n| n.start());
            let list = SegmentList::parse(&text.as_str()[cur.end()..end]);
            let versification = data.chapter_verses().get_chapter_verses(&book);
            let valid = Resolver::for_book(versification).resolve(&list.nodes);
            let parsed = Resolver::default().resolve(&list.nodes);
            if parsed.used <= valid.used {
                continue;
            }
            let node = &list.nodes[valid.used];
            if node.start_verse.is_none() {
                continue;
            }
            let (Some(segment), Some(name)) =
                (parsed.segments.get(valid.used), data.books().get_name(book))
            else {
                continue;
            };
            let bytes = cur.end() + node.start.span.start..cur.end() + node.complete_end();
            // The first chapter, or verse in a chapter, that is past the end
            let detail = versification.and_then(|versification| {
                let chapters = versification.get_chapter_count();
                let ends = [
                    (segment.starting_chapter(), segment.starting_verse()),
                    (
                        segment.ending_chapter(),
                        segment.ending_verse().unwrap_or(0),
                    ),
                ];
                ends.into_iter().find_map(|(chapter, verse)| {
                    if chapter > chapters {
                        let s = if chapters == 1 { "" } else { "s" };
                        return Some(format!("{name} has {chapters} chapter{s}"));
                    }
                    let verses = versification.get_last_verse(chapter)?;
                    (verse > verses).then(|| format!("{name} {chapter} has {verses} verses"))
                })
            });
            // A number too large to hold (`6:280`, read as 255) is quoted as written
            let source = &text.as_str()[bytes.clone()];
            let clamped = node
                .parts()
                .filter_map(|p| p.number)
                .chain([node.start])
                .any(|n| {
                    n.value == TOO_LARGE
                        && &text.as_str()[cur.end()..][n.span.start..n.span.end] != "255"
                });
            let written = if clamped {
                source.to_string()
            } else {
                segment.to_string()
            };
            problems.push(Problem {
                bytes: text.original_range(bytes),
                message: format!("{name} {written} does not exist"),
                detail,
            });
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn says_what_exists() {
        let detail = |text: &str| -> Vec<Option<String>> {
            BibleMatcher::default()
                .problems(text)
                .into_iter()
                .map(|p| p.detail)
                .collect()
        };
        assert_eq!(detail("John 3:99"), [Some("John 3 has 36 verses".into())]);
        assert_eq!(
            detail("John 3:30-40"),
            [Some("John 3 has 36 verses".into())]
        );
        assert_eq!(detail("John 25:1"), [Some("John has 21 chapters".into())]);
        assert_eq!(
            detail("John 21:1-22:3"),
            [Some("John has 21 chapters".into())]
        );
        assert_eq!(
            detail("Obadiah 2:1"),
            [Some("Obadiah has 1 chapter".into())]
        );
        // Numbers too large to be any verse are verses that don't exist, not the end of the
        // reference (which would leave `1 Timothy 6`)
        assert_eq!(
            problems("1 Timothy 6:280"),
            [("6:280".into(), "1 Timothy 6:280 does not exist".into())]
        );
        assert_eq!(
            detail("1 Timothy 6:280"),
            [Some("1 Timothy 6 has 21 verses".into())]
        );
    }

    use super::*;

    fn problems(text: &str) -> Vec<(String, String)> {
        BibleMatcher::default()
            .problems(text)
            .into_iter()
            .map(|p| (text[p.bytes].to_string(), p.message))
            .collect()
    }

    #[test]
    fn reports_segments_that_do_not_exist() {
        assert_eq!(
            problems("See John 3:99."),
            [("3:99".into(), "John 3:99 does not exist".into())]
        );
        assert_eq!(
            problems("John 3:16, 4:99"),
            [("4:99".into(), "John 4:99 does not exist".into())]
        );
        assert_eq!(
            problems("Jude 2:1"),
            [("2:1".into(), "Jude 2:1 does not exist".into())]
        );
    }

    #[test]
    fn ignores_valid_references_and_prose() {
        assert!(problems("John 3:16, Jude 5, Mar 25, 2025, is 1 of").is_empty());
        assert!(problems("Mark 16, 25").is_empty());
    }
}

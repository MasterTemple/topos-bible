use itertools::Itertools;

use crate::{
    data::chapter_verses::ChapterVerses,
    segments::{
        grammar::{Part, SegmentNode},
        segment::Segment,
        verse_bounds::VerseBounds,
    },
};

/// The numbers typed so far are kept (unused for now) to filter suggestions by prefix later
#[allow(dead_code, clippy::enum_variant_names)]
#[derive(Clone, Debug)]
pub(crate) enum IncompleteSegment {
    /// - Example Segment: ""
    /// - Suggests: chapters or verses
    ChapterOrVerse { start: Option<u8> },
    // ChapterOrVerse,
    /// - Example Segment: "1-"
    /// - Suggests: chapters or verses
    ChapterOrVerseTo { start: u8, end: Option<u8> },
    // ChapterTo { start_chapter: u8 },
    /// - Example Segment: "1:"
    /// - Suggests: verses
    ChapterVerse {
        start_chapter: u8,
        start_verse: Option<u8>,
    },
    /// - Example Segment: "1:1-"
    /// - Suggests: chapters or verses
    ChapterVerseTo {
        start_chapter: u8,
        start_verse: u8,
        end: Option<u8>,
    },
    /// - Example Segment: "1-2:"
    /// - Suggests: verses
    ChapterRangeTo {
        start_chapter: u8,
        end_chapter: u8,
        end_verse: Option<u8>,
    },
    /// - Example Segment: "1:1-2:"
    /// - Suggests: verses
    ChapterVerseRangeTo {
        start_chapter: u8,
        start_verse: u8,
        end_chapter: u8,
        end_verse: Option<u8>,
    },
}

impl IncompleteSegment {
    /**
    - Maps the segment that is still being typed (from
      [`SegmentList::split_incomplete`](crate::segments::grammar::SegmentList::split_incomplete)) to what
      should be suggested next
    - [`None`] (no segment started yet) suggests from scratch
    */
    pub fn from_node(node: Option<&SegmentNode>) -> Option<Self> {
        let Some(node) = node else {
            return Some(Self::ChapterOrVerse { start: None });
        };
        let start = node.start.value;
        // Outer `Option`: was the part written? Inner `Option`: does it have a number yet?
        let value = |part: Option<Part>| part.map(|p| p.number.map(|n| n.value));

        Some(
            match (
                value(node.start_verse),
                value(node.end),
                value(node.end_verse),
            ) {
                // Segment: "1"
                (None, None, None) => Self::ChapterOrVerse { start: Some(start) },
                // Segment: "1-" (chapter or verse)
                (None, Some(end), None) => Self::ChapterOrVerseTo { start, end },
                // Segment: "1:" (verse)
                (Some(start_verse), None, None) => Self::ChapterVerse {
                    start_chapter: start,
                    start_verse,
                },
                // Segment: "1:1-" (chapter or verse)
                (Some(Some(start_verse)), Some(end), None) => Self::ChapterVerseTo {
                    start_chapter: start,
                    start_verse,
                    end,
                },
                // Segment: "1-2:" (verse)
                (None, Some(Some(end_chapter)), Some(end_verse)) => Self::ChapterRangeTo {
                    start_chapter: start,
                    end_chapter,
                    end_verse,
                },
                // Segment: "1:1-2:" (verse)
                (Some(Some(start_verse)), Some(Some(end_chapter)), Some(end_verse)) => {
                    Self::ChapterVerseRangeTo {
                        start_chapter: start,
                        start_verse,
                        end_chapter,
                        end_verse,
                    }
                }
                // The parser stops at the first dangling part, so nothing follows one
                _ => return None,
            },
        )
    }

    /**
    The segments that can complete this one, chapters or verses
    - `verse_chapter` is the chapter its first number is a verse in, or [`None`] if that number
      is a chapter ([`Resolver::next_verse_chapter`](crate::segments::resolve::Resolver::next_verse_chapter)):
      `3:3,4,5-` offers verses `5-6` ... in chapter 3, `3:3; 5-` offers chapters `5-6` ...
    - Verses continue after the previous segment's last verse when it is in the same chapter
    */
    pub fn suggest(
        &self,
        chapter_verses: &ChapterVerses,
        prev: Option<&Segment>,
        verse_chapter: Option<u8>,
    ) -> Option<Vec<Segment>> {
        let last_chapter = chapter_verses.get_chapter_count();

        Some(match self.clone() {
            Self::ChapterOrVerse { start: _ } => match verse_chapter {
                Some(chapter) => {
                    let first = match prev {
                        Some(prev) if prev.ending_chapter() == chapter => {
                            prev.ending_verse().map_or(1, |v| v.saturating_add(1))
                        }
                        _ => 1,
                    };
                    (first..=chapter_verses.get_last_verse(chapter)?)
                        .map(|v| Segment::chapter_verse(chapter, v))
                        .collect_vec()
                }
                None => (1..=last_chapter).map(Segment::full_chapter).collect(),
            },

            Self::ChapterOrVerseTo { start, end: _ } => match verse_chapter {
                // A verse range in that chapter: `3:3,4,5-` is 3:5-6, 3:5-7, ...
                Some(chapter) => (start.saturating_add(1)
                    ..=chapter_verses.get_last_verse(chapter)?)
                    .map(|v| Segment::chapter_verse_range(chapter, start, v))
                    .collect_vec(),
                None => (start.saturating_add(1)..=last_chapter)
                    .map(|c| Segment::full_chapter_range(start, c))
                    .collect_vec(),
            },

            Self::ChapterVerse {
                start_chapter,
                start_verse: _,
            } => (1..=chapter_verses.get_last_verse(start_chapter)?)
                .map(|v| Segment::chapter_verse(start_chapter, v))
                .collect_vec(),

            Self::ChapterVerseTo {
                start_chapter,
                start_verse,
                end: _,
            } => {
                let verses = (start_verse.saturating_add(1)
                    ..=chapter_verses.get_last_verse(start_chapter)?)
                    .map(|v| Segment::chapter_verse_range(start_chapter, start_verse, v))
                    .collect_vec();
                let chapters = (start_chapter.saturating_add(1)..=last_chapter)
                    .map(|c| Segment::chapter_range(start_chapter, start_verse, c, 1))
                    .collect_vec();
                verses.into_iter().chain(chapters).collect()
            }

            Self::ChapterRangeTo {
                start_chapter,
                end_chapter,
                end_verse: _,
            } => (1..=chapter_verses.get_last_verse(end_chapter)?)
                .map(|v| Segment::chapter_range(start_chapter, 1, end_chapter, v))
                .collect_vec(),

            Self::ChapterVerseRangeTo {
                start_chapter,
                start_verse,
                end_chapter,
                end_verse: _,
            } => (1..=chapter_verses.get_last_verse(end_chapter)?)
                .map(|v| Segment::chapter_range(start_chapter, start_verse, end_chapter, v))
                .collect_vec(),
        })
    }
}

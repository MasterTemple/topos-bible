use itertools::Itertools;

use crate::{
    data::chapter_verses::ChapterVerses,
    segments::{
        grammar::{Part, SegmentNode},
        segment::Segment,
        verse_bounds::VerseBounds,
    },
};

#[derive(Clone, Debug)]
pub enum IncompleteSegment {
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
    This will suggest segments that can be added to the input segments
    These segments can be suggesting chapters or verses or both
    */
    pub fn suggest(
        &self,
        chapter_verses: &ChapterVerses,
        prev: Option<&Segment>,
    ) -> Option<Vec<Segment>> {
        let last_chapter = chapter_verses.get_chapter_count();

        Some(match self.clone() {
            // the first number of all segments is always and only a chapter
            Self::ChapterOrVerse { start } => {
                if let Some(prev) = prev {
                    let next_verse = match prev.ending_verse() {
                        Some(cur) => cur + 1,
                        None => 1,
                    };
                    let current_chapter = prev.ending_chapter();
                    let next_chapter = current_chapter + 1;
                    let verses = (next_verse..=chapter_verses.get_last_verse(current_chapter)?)
                        .map(|v| Segment::chapter_verse(current_chapter, v))
                        .collect_vec();
                    verses
                } else {
                    (1..=last_chapter)
                        .map(|ch| Segment::full_chapter(ch))
                        .collect()
                }
            }

            Self::ChapterOrVerseTo { start, end } => {
                // I can use context to determine if start is a chapter or a verse
                if let Some(prev) = prev {
                    let is_chapter = prev.ending_verse().is_some();
                    if is_chapter {
                        let chapters = (start + 1..=last_chapter)
                            .map(|c| Segment::full_chapter_range(start, c))
                            .collect_vec();
                        chapters
                    } else {
                        let next_verse = match prev.ending_verse() {
                            Some(cur) => cur + 1,
                            None => 1,
                        };
                        let current_chapter = prev.ending_chapter();
                        let verses = (next_verse
                            ..=chapter_verses.get_last_verse(current_chapter)?)
                            .map(|v| Segment::chapter_verse(current_chapter, v))
                            .collect_vec();
                        verses
                    }
                } else {
                    let chapters = (start + 1..=last_chapter)
                        .map(|c| Segment::full_chapter_range(start, c))
                        .collect_vec();
                    chapters
                }
            }

            Self::ChapterVerse {
                start_chapter,
                start_verse,
            } => {
                let verses = (1..=chapter_verses.get_last_verse(start_chapter)?)
                    .map(|v| Segment::chapter_verse(start_chapter, v))
                    .collect_vec();
                verses
            }

            Self::ChapterVerseTo {
                start_chapter,
                start_verse,
                end,
            } => {
                let verses = (start_verse + 1..=chapter_verses.get_last_verse(start_chapter)?)
                    .map(|v| Segment::chapter_verse_range(start_chapter, start_verse, v))
                    .collect_vec();
                let chapters = (start_chapter + 1..=last_chapter)
                    .map(|c| Segment::chapter_range(start_chapter, start_verse, c, 1))
                    .collect_vec();
                verses.into_iter().chain(chapters).collect()
            }

            Self::ChapterRangeTo {
                start_chapter,
                end_chapter,
                end_verse,
            } => {
                let verses = (1..=chapter_verses.get_last_verse(end_chapter)?)
                    .map(|v| Segment::chapter_range(start_chapter, 1, end_chapter, v))
                    .collect_vec();
                verses
            }

            Self::ChapterVerseRangeTo {
                start_chapter,
                start_verse,
                end_chapter,
                end_verse,
            } => {
                let verses = (1..=chapter_verses.get_last_verse(end_chapter)?)
                    .map(|v| Segment::chapter_range(start_chapter, start_verse, end_chapter, v))
                    .collect_vec();
                verses
            }
        })
    }
}

use derive_more::{Deref, DerefMut, IntoIterator};
use serde::{Deserialize, Serialize};

use crate::{
    data::books::BookId,
    segments::{
        grammar::{SegmentList, SegmentNode},
        segment::{ChapterlessFormat, Segment},
        verse_bounds::VerseBounds,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Passage {
    pub book: BookId,
    pub segments: Segments,
}

impl Passage {
    pub fn overlaps_with(&self, other: &Passage) -> bool {
        if self.book != other.book {
            return false;
        }
        self.segments.contains_overlap(&other.segments)
    }

    /// Whether every verse of `other` is inside this passage
    pub fn contains(&self, other: &Passage) -> bool {
        self.book == other.book && self.segments.fully_contains(&other.segments)
    }
}

/// TODO: I need Segments and PartialSegments/Incomplete segments to be unified under a large
/// Segment type that I can use for auto-completions
#[derive(Clone, Debug, Deref, DerefMut, Serialize, Deserialize, IntoIterator)]
pub struct Segments(pub Vec<Segment>);

impl Default for Segments {
    fn default() -> Self {
        Self::new()
    }
}

impl Segments {
    pub fn new() -> Self {
        Self(vec![])
    }

    // pub fn overlaps_segment(&self, other: impl Into<Segment>) -> bool {
    pub fn overlaps_with(&self, other: &impl VerseBounds) -> bool {
        self.iter().any(|this| this.overlaps_with(other))
    }

    /// - This can be better optimized, but that is not a priority right now
    /// - I just need some way to order the segments and do it in linear time
    pub fn contains_overlap(&self, other: &Segments) -> bool {
        self.iter().any(|this| other.overlaps_with(this))
    }

    /**
    Whether every segment of `other` lies inside one of these segments
    - A segment covered only by the union of several segments (`1:1-5,6-10` covering `1:3-8`) is not
      detected yet, since open-ended chapters need versification data to merge
    */
    pub fn fully_contains(&self, other: &Segments) -> bool {
        other
            .iter()
            .all(|o| self.iter().any(|this| this.fully_contains(o)))
    }

    pub fn with_book(self, book_id: BookId) -> Passage {
        Passage {
            book: book_id,
            segments: self,
        }
    }

    pub fn with_suggestion(&self, segment: Segment) -> Self {
        let mut new = self.clone();
        new.push(segment);
        new
    }
}

impl Segments {
    pub fn format(&self, verse_seperator: &str, chapter_seperator: &str) -> String {
        let mut prev_chapter = None;
        let mut output = String::new();
        for seg in self.iter() {
            let is_cross_chapter_segment = seg.starting_chapter() != seg.ending_chapter();
            let current_chapter = seg.ending_chapter();
            if let Some(chapter) = prev_chapter {
                if is_cross_chapter_segment || chapter == current_chapter {
                    output.push_str(verse_seperator);
                    output.push_str(&seg.chapterless_format());
                } else {
                    output.push_str(chapter_seperator);
                    output.push_str(&seg.to_string());
                }
            } else {
                output.push_str(&seg.to_string());
            }
            prev_chapter = Some(current_chapter);
        }
        output
    }
}

impl Segments {
    /// Parses as many segments as possible from the start of the input, ignoring the rest
    pub fn parse(segment_window: &str) -> Option<Self> {
        let list = SegmentList::parse(segment_window);
        (!list.is_empty()).then(|| Segments::from(&list))
    }
}

impl std::str::FromStr for Segments {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| String::from("Failed to parse segments"))
    }
}

impl std::fmt::Display for Segments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format(",", "; "))
    }
}

impl From<&SegmentList> for Segments {
    fn from(list: &SegmentList) -> Self {
        Segments::from_nodes(&list.nodes)
    }
}

impl Segments {
    /**
    Resolves parsed nodes into chapters and verses
    - Dangling parts (the `:` in `1:`) are ignored
    - A number without an explicit chapter belongs to the chapter the previous segment ended in
    */
    pub fn from_nodes(nodes: &[SegmentNode]) -> Self {
        let mut segments = Segments::new();
        for node in nodes {
            let start = node.start.value;
            let prev_chapter = segments.last().map(|prev| prev.ending_chapter());
            let new = match (node.start_verse_value(), node.end_value()) {
                // `1:2-3:4`
                (Some(start_verse), Some((end_chapter, Some(end_verse)))) => {
                    Segment::chapter_range(start, start_verse, end_chapter, end_verse)
                }
                // `1:2-3`
                (Some(start_verse), Some((end_verse, None))) => {
                    Segment::chapter_verse_range(start, start_verse, end_verse)
                }
                // `1:2`
                (Some(start_verse), None) => Segment::chapter_verse(start, start_verse),
                (None, Some((end_chapter, Some(end_verse)))) => match prev_chapter {
                    // `5:7, 12-6:6` (verse 12 of chapter 5 to 6:6)
                    Some(chapter) => Segment::chapter_range(chapter, start, end_chapter, end_verse),
                    // `1-2:3`
                    None => Segment::chapter_range(start, 1, end_chapter, end_verse),
                },
                (None, Some((end, None))) => match prev_chapter {
                    // `3:1, 4-5`
                    Some(chapter) => Segment::chapter_verse_range(chapter, start, end),
                    // `1-25`
                    None => Segment::full_chapter_range(start, end),
                },
                (None, None) => match prev_chapter {
                    // `1:1, 3`
                    Some(chapter) => Segment::chapter_verse(chapter, start),
                    // `1`
                    None => Segment::full_chapter(start),
                },
            };
            segments.push(new);
        }
        segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Vec<Segment> {
        Segments::parse(input).unwrap().0
    }

    #[test]
    fn resolves_each_kind() {
        assert_eq!(parse("1:2"), [Segment::chapter_verse(1, 2)]);
        assert_eq!(parse("1.2"), [Segment::chapter_verse(1, 2)]);
        assert_eq!(parse("1:2-3"), [Segment::chapter_verse_range(1, 2, 3)]);
        assert_eq!(parse("1:2-3:4"), [Segment::chapter_range(1, 2, 3, 4)]);
        assert_eq!(parse("1"), [Segment::full_chapter(1)]);
        assert_eq!(parse("1-2"), [Segment::full_chapter_range(1, 2)]);
        assert_eq!(parse("i:ii"), [Segment::chapter_verse(1, 2)]);
    }

    #[test]
    fn ignores_dangling_parts() {
        assert_eq!(parse("3:"), [Segment::full_chapter(3)]);
        assert_eq!(parse("1:1-"), [Segment::chapter_verse(1, 1)]);
        assert_eq!(parse("1:1, "), [Segment::chapter_verse(1, 1)]);
    }

    #[test]
    fn combined() {
        assert_eq!(
            parse("5:1-3,5,7-9,12-6:6,7:7-8:8"),
            [
                Segment::chapter_verse_range(5, 1, 3),
                Segment::chapter_verse(5, 5),
                Segment::chapter_verse_range(5, 7, 9),
                Segment::chapter_range(5, 12, 6, 6),
                Segment::chapter_range(7, 7, 8, 8),
            ]
        );
    }

    #[test]
    fn containment() {
        let segs = |s: &str| Segments::parse(s).unwrap();
        assert!(segs("3").fully_contains(&segs("3:16-18")));
        assert!(segs("3, 1:1-5").fully_contains(&segs("3:16, 1:2")));
        assert!(!segs("3:16-18").fully_contains(&segs("3")));
        assert!(!segs("3:16").fully_contains(&segs("3:16-17")));

        let john = |s: &str| segs(s).with_book(BookId(43));
        assert!(john("3").contains(&john("3:16")));
        assert!(!john("3:16").contains(&john("3:16-18")));
        assert!(!john("3").contains(&segs("3:16").with_book(BookId(1))));
    }

    #[test]
    fn sorts_by_position() {
        let mut segs = segs_vec("5:1, 1:1-3, 1, 1:1");
        segs.sort();
        assert_eq!(
            segs,
            [
                Segment::chapter_verse(1, 1),
                Segment::chapter_verse_range(1, 1, 3),
                Segment::full_chapter(1),
                Segment::chapter_verse(5, 1),
            ]
        );
    }

    fn segs_vec(input: &str) -> Vec<Segment> {
        // Resolve each comma-separated part on its own so bare numbers stay chapters
        input.split(", ").map(|s| parse(s)[0]).collect()
    }

    #[test]
    fn nothing_to_parse() {
        assert!(Segments::parse("").is_none());
        assert!(Segments::parse("and then").is_none());
    }
}

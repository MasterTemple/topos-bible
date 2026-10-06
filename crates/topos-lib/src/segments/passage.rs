use derive_more::{Deref, DerefMut, IntoIterator};
use serde::{Deserialize, Serialize};

use crate::{
    data::books::BookId,
    segments::{
        grammar::{SegmentList, SegmentNode},
        resolve::Resolver,
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
    /// Writes segments compactly, dropping the chapter wherever the result still parses back the
    /// same way (see [`Resolver`] for those rules)
    pub fn format(&self, verse_separator: &str, chapter_separator: &str) -> String {
        let mut output = String::new();
        let mut prev: Option<&Segment> = None;
        for seg in self.iter() {
            match prev {
                None => output.push_str(&seg.to_string()),
                Some(prev) if Self::continues(prev, seg) => {
                    output.push_str(verse_separator);
                    output.push_str(&seg.chapterless_format());
                }
                Some(_) => {
                    output.push_str(chapter_separator);
                    output.push_str(&seg.to_string());
                }
            }
            prev = Some(seg);
        }
        output
    }

    /// Whether `seg` can be written without its chapter after `prev`
    fn continues(prev: &Segment, seg: &Segment) -> bool {
        match prev.ending_verse() {
            // After a verse, a bare number is a verse in the same chapter
            Some(_) => {
                seg.starting_chapter() == prev.ending_chapter()
                    && matches!(
                        seg,
                        Segment::ChapterVerse(_)
                            | Segment::ChapterVerseRange(_)
                            | Segment::ChapterRange(_)
                    )
            }
            // After whole chapters, a bare number is another chapter
            None => matches!(seg, Segment::FullChapter(_) | Segment::FullChapterRange(_)),
        }
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
    /// Resolves parsed nodes without book data (see [`Resolver`] for the rules)
    pub fn from_nodes(nodes: &[SegmentNode]) -> Self {
        Resolver::default().resolve(nodes).segments
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
        assert_eq!(parse("ii:3"), [Segment::chapter_verse(2, 3)]);
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
    fn display_round_trips() {
        for (input, expected) in [
            ("5:1-3,5,7-9,12-6:6,7:7-8:8", "5:1-3,5,7-9,12-6:6; 7:7-8:8"),
            ("1, 3", "1,3"),
            ("1-2, 4-5", "1-2,4-5"),
            ("1, 3:16, 18", "1; 3:16,18"),
            ("3:16; 4", "3:16; 4"),
            ("1-2:3, 5", "1:1-2:3,5"),
            ("1:1-2:5, 3:1", "1:1-2:5; 3:1"),
        ] {
            let segments = Segments::parse(input).unwrap();
            let formatted = segments.to_string();
            assert_eq!(formatted, expected, "{input}");
            assert_eq!(
                Segments::parse(&formatted).unwrap().0,
                segments.0,
                "{input}"
            );
        }
    }

    #[test]
    fn nothing_to_parse() {
        assert!(Segments::parse("").is_none());
        assert!(Segments::parse("and then").is_none());
    }
}

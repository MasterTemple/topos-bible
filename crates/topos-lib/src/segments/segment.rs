use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};

use crate::segments::{
    passage::Segments,
    units::{
        chapter_range::ChapterRange, chapter_verse::ChapterVerse,
        chapter_verse_range::ChapterVerseRange, full_chapter::FullChapter,
        full_chapter_range::FullChapterRange, full_chapter_verse_range::FullChapterVerseRange,
    },
    verse_bounds::VerseBounds,
};

/// Remember, these correspond to
/// ```text
///                   `John 1,2-4,5:1-3,5,7-9,12-6:6,7:7-8:8`
///                        | |   |     | |   |      |       |
/// -----------------------+ |   |     | |   |      |       |
/// Full Chapter:        `1` |   |     | |   |      |       |
/// -------------------------+   |     | |   |      |       |
/// Full Chapter Range:  `2-4`   |     | |   |      |       |
/// -----------------------------+     | |   |      |       |
/// Chapter Range:       `5:1-3`       | |   |      |       |
/// -----------------------------------+ |   |      |       |
/// Chapter Verse:       `5:5            |   |      |       |
/// -------------------------------------+   |      |       |
/// Chapter Verse Range: `5:7-9`             |      |       |
/// -----------------------------------------+      |       |
/// Chapter Range:       `5:12-6:6`                 |       |
/// ------------------------------------------------+       |
/// Chapter Range:       `7:7-8:8`                          |
/// --------------------------------------------------------+
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Segment {
    /// - This is a single chapter/verse reference
    /// - Ex: `1:2` in `John 1:2`
    ChapterVerse(ChapterVerse),
    /// - This is a range of verse references within a single chapter
    /// - Ex: `1:2-3` `John 1:2-3`
    ChapterVerseRange(ChapterVerseRange),
    /// - This is a range of verse references across a multiple chapters
    /// - Ex: `John 1:2-3:4`
    ChapterRange(ChapterRange),
    /// - This is a single chapter reference
    /// - Ex: `1` in `John 1`
    FullChapter(FullChapter),
    /// - This is a full chapter range reference
    /// - Ex: `1-2` in `John 1-2`
    FullChapterRange(FullChapterRange),
    /// - This is a full chapter to chapter-verse range reference
    /// - Ex: `1-2:3` in `John 1-2:3`
    // Actually this might not even have to exist if I have `FullChapterVerseRange` on
    // `VerboseSegmentPair`
    FullChapterVerseRange(FullChapterVerseRange),
}

/// Orders by where the segment starts, then where it ends (an open end sorts after any verse)
impl Ord for Segment {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let key = |s: &Self| {
            (
                s.starting_chapter(),
                s.starting_verse(),
                s.ending_chapter(),
                s.ending_verse().map_or(u16::MAX, u16::from),
            )
        };
        // Different variants can share bounds (`1-2:3` and `1:1-2:3` cover the same verses), so
        // fall back to the variant to stay consistent with `Eq`
        key(self)
            .cmp(&key(other))
            .then_with(|| self.variant_index().cmp(&other.variant_index()))
    }
}

impl PartialOrd for Segment {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Segment {
    fn variant_index(&self) -> u8 {
        match self {
            Segment::ChapterVerse(_) => 0,
            Segment::ChapterVerseRange(_) => 1,
            Segment::ChapterRange(_) => 2,
            Segment::FullChapter(_) => 3,
            Segment::FullChapterRange(_) => 4,
            Segment::FullChapterVerseRange(_) => 5,
        }
    }
}

impl VerseBounds for Segment {
    fn starting_chapter(&self) -> u8 {
        match self {
            Segment::ChapterVerse(chapter_verse) => chapter_verse.starting_chapter(),
            Segment::ChapterVerseRange(chapter_range) => chapter_range.starting_chapter(),
            Segment::ChapterRange(book_range) => book_range.starting_chapter(),
            Segment::FullChapter(full_chapter) => full_chapter.starting_chapter(),
            Segment::FullChapterRange(full_chapter_range) => full_chapter_range.starting_chapter(),
            Segment::FullChapterVerseRange(v) => v.starting_chapter(),
        }
    }

    fn starting_verse(&self) -> u8 {
        match self {
            Segment::ChapterVerse(chapter_verse) => chapter_verse.starting_verse(),
            Segment::ChapterVerseRange(chapter_range) => chapter_range.starting_verse(),
            Segment::ChapterRange(book_range) => book_range.starting_verse(),
            Segment::FullChapter(full_chapter) => full_chapter.starting_verse(),
            Segment::FullChapterRange(full_chapter_range) => full_chapter_range.starting_verse(),
            Segment::FullChapterVerseRange(v) => v.starting_verse(),
        }
    }

    fn ending_chapter(&self) -> u8 {
        match self {
            Segment::ChapterVerse(chapter_verse) => chapter_verse.ending_chapter(),
            Segment::ChapterVerseRange(chapter_range) => chapter_range.ending_chapter(),
            Segment::ChapterRange(book_range) => book_range.ending_chapter(),
            Segment::FullChapter(full_chapter) => full_chapter.ending_chapter(),
            Segment::FullChapterRange(full_chapter_range) => full_chapter_range.ending_chapter(),
            Segment::FullChapterVerseRange(v) => v.ending_chapter(),
        }
    }

    fn ending_verse(&self) -> Option<u8> {
        match self {
            Segment::ChapterVerse(chapter_verse) => chapter_verse.ending_verse(),
            Segment::ChapterVerseRange(chapter_range) => chapter_range.ending_verse(),
            Segment::ChapterRange(book_range) => book_range.ending_verse(),
            Segment::FullChapter(full_chapter) => full_chapter.ending_verse(),
            Segment::FullChapterRange(full_chapter_range) => full_chapter_range.ending_verse(),
            Segment::FullChapterVerseRange(v) => v.ending_verse(),
        }
    }
}

// Easy constructors
impl Segment {
    pub fn chapter_verse(chapter: u8, verse: u8) -> Self {
        Self::ChapterVerse(ChapterVerse::new(chapter, verse))
    }

    pub fn chapter_verse_range(chapter: u8, start_verse: u8, end_verse: u8) -> Self {
        let cvr = ChapterVerseRange::new(chapter, start_verse, end_verse);
        if let Some(cv) = cvr.as_chapter_verse() {
            Self::ChapterVerse(cv)
        } else {
            Self::ChapterVerseRange(cvr)
        }
    }

    pub fn chapter_range(
        start_chapter: u8,
        start_verse: u8,
        end_chapter: u8,
        end_verse: u8,
    ) -> Self {
        let cr = ChapterRange::new(start_chapter, start_verse, end_chapter, end_verse);
        if let Some(ChapterVerseRange { chapter, verses }) = cr.as_chapter_verse_range() {
            Self::chapter_verse_range(chapter, verses.start, verses.end)
        } else {
            Self::ChapterRange(cr)
        }
    }

    pub fn full_chapter(chapter: u8) -> Self {
        Self::FullChapter(FullChapter::new(chapter))
    }

    pub fn full_chapter_range(start: u8, end: u8) -> Self {
        Self::FullChapterRange(FullChapterRange::new(start, end))
    }

    pub fn as_segments(self) -> Segments {
        Segments(vec![self])
    }

    pub fn is_range(&self) -> bool {
        match self {
            Segment::ChapterVerse(_) | Segment::FullChapter(_) => false,
            Segment::ChapterVerseRange(_)
            | Segment::ChapterRange(_)
            | Segment::FullChapterRange(_)
            | Segment::FullChapterVerseRange(_) => true,
        }
    }
}

pub trait ChapterlessFormat {
    fn chapterless_format(&self) -> String;
}

impl ChapterlessFormat for Segment {
    fn chapterless_format(&self) -> String {
        match self {
            Segment::ChapterVerse(chapter_verse) => chapter_verse.chapterless_format(),
            Segment::ChapterVerseRange(chapter_verse_range) => {
                chapter_verse_range.chapterless_format()
            }
            Segment::ChapterRange(chapter_range) => chapter_range.chapterless_format(),
            Segment::FullChapter(full_chapter) => full_chapter.chapterless_format(),
            Segment::FullChapterRange(full_chapter_range) => {
                full_chapter_range.chapterless_format()
            }
            Segment::FullChapterVerseRange(v) => v.chapterless_format(),
        }
    }
}

// Formatting
impl Display for Segment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Segment::ChapterVerse(chapter_verse) => chapter_verse.to_string(),
                Segment::ChapterVerseRange(chapter_verse_range) => chapter_verse_range.to_string(),
                Segment::ChapterRange(chapter_range) => chapter_range.to_string(),
                Segment::FullChapter(full_chapter) => full_chapter.to_string(),
                Segment::FullChapterRange(full_chapter_range) => full_chapter_range.to_string(),
                Segment::FullChapterVerseRange(v) => v.to_string(),
            }
        )
    }
}

impl Segment {
    pub fn parse(input: &str) -> Result<Self, String> {
        input.parse::<Self>()
    }
}

impl std::str::FromStr for Segment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let segments =
            Segments::parse(s).ok_or_else(|| "Could not parse any segments.".to_string())?;
        if segments.is_empty() {
            Err(String::from("No segments found"))?
        }
        if segments.len() == 1 {
            Ok(segments[0])
        } else {
            Err(String::from("Multiple segments found"))?
        }
    }
}

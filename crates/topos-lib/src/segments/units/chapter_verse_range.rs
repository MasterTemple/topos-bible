use super::range_pair::RangePair;
use crate::segments::{
    segment::{ChapterlessFormat, Segment},
    units::chapter_verse::ChapterVerse,
    units::parse::{ParsableSegment, SegmentParseMethods},
    verse_bounds::VerseBounds,
};
use serde::{Deserialize, Serialize, de::Visitor};
use std::{fmt::Display, str::FromStr};

/// - This is a range of verse references within a single chapter
/// - Ex: `1:2-3` `John 1:2-3`
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChapterVerseRange {
    pub chapter: u8,
    pub verses: RangePair<u8>,
}

impl ChapterVerseRange {
    pub fn as_chapter_verse(&self) -> Option<ChapterVerse> {
        if self.verses.start == self.verses.end {
            Some(ChapterVerse::new(self.chapter, self.verses.start))
        } else {
            None
        }
    }
}

impl Display for ChapterVerseRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(cv) = self.as_chapter_verse() {
            cv.fmt(f)
        } else {
            write!(
                f,
                "{}:{}-{}",
                self.chapter, self.verses.start, self.verses.end
            )
        }
    }
}

impl ChapterlessFormat for ChapterVerseRange {
    fn chapterless_format(&self) -> String {
        if let Some(cv) = self.as_chapter_verse() {
            cv.chapterless_format()
        } else {
            format!("{}-{}", self.verses.start, self.verses.end)
        }
    }
}

impl Serialize for ChapterVerseRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

struct ChapterVerseRangeVisitor;

impl<'de> Visitor<'de> for ChapterVerseRangeVisitor {
    type Value = ChapterVerseRange;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("format '{}:{}-{}'")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        Ok(ChapterVerseRange::new(
            seq.next_element()?
                .ok_or_else(|| serde::de::Error::custom("missing chapter"))?,
            seq.next_element()?
                .ok_or_else(|| serde::de::Error::custom("missing end chapter"))?,
            seq.next_element()?
                .ok_or_else(|| serde::de::Error::custom("missing end verse"))?,
        ))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse().map_err(|e| E::custom(e))
    }
}

impl<'de> Deserialize<'de> for ChapterVerseRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(ChapterVerseRangeVisitor)
    }
}

impl FromStr for ChapterVerseRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl VerseBounds for ChapterVerseRange {
    fn starting_chapter(&self) -> u8 {
        self.chapter
    }

    fn starting_verse(&self) -> u8 {
        self.verses.start
    }

    fn ending_chapter(&self) -> u8 {
        self.chapter
    }

    fn ending_verse(&self) -> Option<u8> {
        Some(self.verses.end)
    }
}

impl ChapterVerseRange {
    pub fn new(chapter: u8, start_verse: u8, end_verse: u8) -> Self {
        ChapterVerseRange {
            chapter,
            verses: RangePair {
                start: start_verse,
                end: end_verse,
            },
        }
    }
}

impl Into<Segment> for ChapterVerseRange {
    fn into(self) -> Segment {
        Segment::ChapterVerseRange(self)
    }
}

impl TryFrom<Segment> for ChapterVerseRange {
    type Error = String;

    fn try_from(value: Segment) -> Result<Self, Self::Error> {
        Ok(match value {
            Segment::ChapterVerse(chapter_verse) => ChapterVerseRange::new(
                chapter_verse.chapter,
                chapter_verse.verse,
                chapter_verse.verse,
            ),
            Segment::ChapterVerseRange(chapter_verse_range) => chapter_verse_range,
            Segment::ChapterRange(_) => {
                Err(format!("Cannot coerce ChapterRange into ChapterVerseRange"))?
            }
            Segment::FullChapter(_) => {
                Err(format!("Cannot coerce FullChapter into ChapterVerseRange"))?
            }
            Segment::FullChapterRange(_) => Err(format!(
                "Cannot coerce FullChapterRange into ChapterVerseRange"
            ))?,
            Segment::FullChapterVerseRange(_) => Err(format!(
                "Cannot coerce FullChapterVerseRange into ChapterVerseRange"
            ))?,
        })
    }
}

impl ParsableSegment for ChapterVerseRange {
    const EXPECTED_FORMAT: &'static str = "{}:{}-{}";

    fn parse_strict(input: &str) -> Result<Self, String> {
        let chars = &mut input.chars().peekable();

        let chapter = ChapterVerseRange::take_number(chars)?;
        ChapterVerseRange::expect_char(chars, ':')?;
        let start_verse = ChapterVerseRange::take_number(chars)?;
        ChapterVerseRange::expect_char(chars, '-')?;
        let end_verse = ChapterVerseRange::take_number(chars)?;
        ChapterVerseRange::expect_done(chars)?;

        Ok(ChapterVerseRange::new(chapter, start_verse, end_verse))
    }
}

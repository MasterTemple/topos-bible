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
pub struct FullChapterVerseRange {
    pub start: u8,
    pub end: ChapterVerse,
}

impl Display for FullChapterVerseRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}:{}", self.start, self.end.chapter, self.end.verse)
    }
}

impl ChapterlessFormat for FullChapterVerseRange {
    fn chapterless_format(&self) -> String {
        format!("{}:{}", self.end.chapter, self.end.verse)
    }
}

impl Serialize for FullChapterVerseRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

struct ChapterVerseRangeVisitor;

impl<'de> Visitor<'de> for ChapterVerseRangeVisitor {
    type Value = FullChapterVerseRange;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("format '{}-{}:{}'")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        Ok(FullChapterVerseRange::new(
            seq.next_element()?
                .ok_or_else(|| serde::de::Error::custom("missing start chapter"))?,
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

impl<'de> Deserialize<'de> for FullChapterVerseRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(ChapterVerseRangeVisitor)
    }
}

impl FromStr for FullChapterVerseRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl VerseBounds for FullChapterVerseRange {
    fn starting_chapter(&self) -> u8 {
        self.start
    }

    fn starting_verse(&self) -> u8 {
        1
    }

    fn ending_chapter(&self) -> u8 {
        self.end.chapter
    }

    fn ending_verse(&self) -> Option<u8> {
        Some(self.end.verse)
    }
}

impl FullChapterVerseRange {
    pub fn new(start_chapter: u8, end_chapter: u8, end_verse: u8) -> Self {
        FullChapterVerseRange {
            start: start_chapter,
            end: ChapterVerse {
                chapter: end_chapter,
                verse: end_verse,
            },
        }
    }
}

impl Into<Segment> for FullChapterVerseRange {
    fn into(self) -> Segment {
        Segment::FullChapterVerseRange(self)
    }
}

impl TryFrom<Segment> for FullChapterVerseRange {
    type Error = String;

    fn try_from(value: Segment) -> Result<Self, Self::Error> {
        Ok(match value {
            Segment::ChapterVerse(chapter_verse) => FullChapterVerseRange::new(
                chapter_verse.chapter,
                chapter_verse.verse,
                chapter_verse.verse,
            ),
            Segment::ChapterVerseRange(_) => Err(format!(
                "Cannot coerce ChapterRange into FullChapterVerseRange"
            ))?,
            Segment::ChapterRange(_) => Err(format!(
                "Cannot coerce ChapterRange into FullChapterVerseRange"
            ))?,
            Segment::FullChapter(_) => Err(format!(
                "Cannot coerce FullChapter into FullChapterVerseRange"
            ))?,
            Segment::FullChapterRange(_) => Err(format!(
                "Cannot coerce FullChapterRange into FullChapterVerseRange"
            ))?,
            Segment::FullChapterVerseRange(full_chapter_verse_range) => full_chapter_verse_range,
        })
    }
}

impl ParsableSegment for FullChapterVerseRange {
    const EXPECTED_FORMAT: &'static str = "{}-{}:{}";

    fn parse_strict(input: &str) -> Result<Self, String> {
        let chars = &mut input.chars().peekable();

        let chapter = FullChapterVerseRange::take_number(chars)?;
        FullChapterVerseRange::expect_char(chars, '-')?;
        let end_chapter = FullChapterVerseRange::take_number(chars)?;
        FullChapterVerseRange::expect_char(chars, ':')?;
        let end_verse = FullChapterVerseRange::take_number(chars)?;
        FullChapterVerseRange::expect_done(chars)?;

        Ok(FullChapterVerseRange::new(chapter, end_chapter, end_verse))
    }
}

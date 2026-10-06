use crate::{
    error::ToposError,
    segments::{
        segment::Segment,
        units::chapter_verse::ChapterVerse,
        units::parse::{ParsableSegment, SegmentParseMethods},
        verse_bounds::VerseBounds,
    },
};
use serde::{Deserialize, Serialize, de::Visitor};
use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

/// - This is a single chapter reference
/// - Ex: `1` in `John 1`
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FullChapter {
    pub chapter: u8,
}

impl Display for FullChapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.chapter)
    }
}

impl Serialize for FullChapter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

struct FullChapterVisitor;

impl<'de> Visitor<'de> for FullChapterVisitor {
    type Value = FullChapter;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("format '{}'")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        Ok(FullChapter::new(seq.next_element()?.ok_or_else(|| {
            serde::de::Error::custom("missing chapter")
        })?))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        v.parse().map_err(|e| E::custom(e))
    }
}

impl<'de> Deserialize<'de> for FullChapter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(FullChapterVisitor)
    }
}

impl FromStr for FullChapter {
    type Err = ToposError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl VerseBounds for FullChapter {
    fn starting_chapter(&self) -> u8 {
        self.chapter
    }

    fn starting_verse(&self) -> u8 {
        1
    }

    fn ending_chapter(&self) -> u8 {
        self.chapter
    }

    fn ending_verse(&self) -> Option<u8> {
        None
    }
}

impl FullChapter {
    pub fn new(chapter: u8) -> Self {
        FullChapter { chapter }
    }
    /// Ex: `Jude 3` is actually `Jude 1:3`
    pub fn as_single_chapter_book_verse(&self) -> ChapterVerse {
        let verse = self.chapter;
        ChapterVerse::new(1, verse)
    }
}

impl From<FullChapter> for Segment {
    fn from(val: FullChapter) -> Self {
        Segment::FullChapter(val)
    }
}

impl TryFrom<Segment> for FullChapter {
    type Error = ToposError;

    fn try_from(value: Segment) -> Result<Self, Self::Error> {
        Ok(match value {
            Segment::ChapterVerse(chapter_verse) => FullChapter::new(chapter_verse.chapter),
            Segment::ChapterVerseRange(chapter_verse_range) => {
                FullChapter::new(chapter_verse_range.chapter)
            }
            Segment::ChapterRange(_) => Err(ToposError::Coerce {
                from: "ChapterRange",
                to: "FullChapter",
            })?,
            Segment::FullChapter(full_chapter) => full_chapter,
            Segment::FullChapterRange(_) => Err(ToposError::Coerce {
                from: "FullChapterRange",
                to: "FullChapter",
            })?,
            Segment::FullChapterVerseRange(_) => Err(ToposError::Coerce {
                from: "FullChapterVerseRange",
                to: "FullChapter",
            })?,
        })
    }
}

impl ParsableSegment for FullChapter {
    const EXPECTED_FORMAT: &'static str = "{}";

    fn parse_strict(input: &str) -> Result<Self, ToposError> {
        let chars = &mut input.chars().peekable();

        let chapter = FullChapter::take_number(chars)?;
        FullChapter::expect_done(chars)?;

        Ok(FullChapter::new(chapter))
    }
}

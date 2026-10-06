use crate::segments::{
    segment::{ChapterlessFormat, Segment},
    units::chapter_verse_range::ChapterVerseRange,
    units::parse::{ParsableSegment, SegmentParseMethods},
    verse_bounds::VerseBounds,
};

use super::{chapter_verse::ChapterVerse, range_pair::RangePair};
use serde::{Deserialize, Serialize, de::Visitor};
use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    str::FromStr,
};

/// - This is a range of verse references across a multiple chapters
/// - Ex: `1:2-3:4` in `John 1:2-3:4`
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChapterRange(RangePair<ChapterVerse>);

impl ChapterRange {
    pub fn as_chapter_verse_range(&self) -> Option<ChapterVerseRange> {
        if self.start.chapter == self.end.chapter {
            Some(ChapterVerseRange::new(
                self.start.chapter,
                self.start.verse,
                self.end.verse,
            ))
        } else {
            None
        }
    }
}

impl Display for ChapterRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(cvr) = self.as_chapter_verse_range() {
            cvr.fmt(f)
        } else {
            write!(
                f,
                "{}:{}-{}:{}",
                self.start.chapter, self.start.verse, self.end.chapter, self.end.verse
            )
        }
    }
}

impl ChapterlessFormat for ChapterRange {
    fn chapterless_format(&self) -> String {
        if let Some(cvr) = self.as_chapter_verse_range() {
            cvr.chapterless_format()
        } else {
            format!(
                "{}-{}:{}",
                self.start.verse, self.end.chapter, self.end.verse
            )
        }
    }
}

impl Serialize for ChapterRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

struct ChapterRangeVisitor;

impl<'de> Visitor<'de> for ChapterRangeVisitor {
    type Value = ChapterRange;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("format '{}:{}-{}:{}'")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        Ok(ChapterRange::new(
            seq.next_element()?
                .ok_or_else(|| serde::de::Error::custom("missing start chapter"))?,
            seq.next_element()?
                .ok_or_else(|| serde::de::Error::custom("missing start verse"))?,
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

impl<'de> Deserialize<'de> for ChapterRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(ChapterRangeVisitor)
    }
}

impl FromStr for ChapterRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl VerseBounds for ChapterRange {
    fn starting_chapter(&self) -> u8 {
        self.start.chapter
    }

    fn starting_verse(&self) -> u8 {
        self.start.verse
    }

    fn ending_chapter(&self) -> u8 {
        self.end.chapter
    }

    fn ending_verse(&self) -> Option<u8> {
        Some(self.end.verse)
    }
}

impl Deref for ChapterRange {
    type Target = RangePair<ChapterVerse>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ChapterRange {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ChapterRange {
    pub fn new(start_chapter: u8, start_verse: u8, end_chapter: u8, end_verse: u8) -> Self {
        ChapterRange(RangePair {
            start: ChapterVerse::new(start_chapter, start_verse),
            end: ChapterVerse::new(end_chapter, end_verse),
        })
    }
}

impl From<ChapterRange> for Segment {
    fn from(val: ChapterRange) -> Self {
        Segment::ChapterRange(val)
    }
}

impl TryFrom<Segment> for ChapterRange {
    type Error = String;

    fn try_from(value: Segment) -> Result<Self, Self::Error> {
        Ok(match value {
            Segment::ChapterVerse(chapter_verse) => ChapterRange::new(
                chapter_verse.chapter,
                chapter_verse.verse,
                chapter_verse.chapter,
                chapter_verse.verse,
            ),
            Segment::ChapterVerseRange(chapter_verse_range) => ChapterRange::new(
                chapter_verse_range.chapter,
                chapter_verse_range.verses.start,
                chapter_verse_range.chapter,
                chapter_verse_range.verses.end,
            ),
            Segment::ChapterRange(chapter_range) => chapter_range,
            Segment::FullChapter(_) => {
                Err("Cannot coerce FullChapter into ChapterRange".to_string())?
            }
            Segment::FullChapterRange(_) => {
                Err("Cannot coerce FullChapterRange into ChapterRange".to_string())?
            }
            Segment::FullChapterVerseRange(_) => {
                Err("Cannot coerce FullChapterVerseRange into ChapterRange".to_string())?
            }
        })
    }
}

impl ParsableSegment for ChapterRange {
    const EXPECTED_FORMAT: &'static str = "{}:{}-{}:{}";

    fn parse_strict(input: &str) -> Result<Self, String> {
        let chars = &mut input.chars().peekable();

        let start_chapter = ChapterRange::take_number(chars)?;
        ChapterRange::expect_char(chars, ':')?;
        let start_verse = ChapterRange::take_number(chars)?;
        ChapterRange::expect_char(chars, '-')?;
        let end_chapter = ChapterRange::take_number(chars)?;
        ChapterRange::expect_char(chars, ':')?;
        let end_verse = ChapterRange::take_number(chars)?;
        ChapterRange::expect_done(chars)?;

        Ok(ChapterRange::new(
            start_chapter,
            start_verse,
            end_chapter,
            end_verse,
        ))
    }
}

// #[cfg(test)]
// mod chapter_range_tests {
//     use std::collections::BTreeMap;
//
//     use serde_json::json;
//
//     use crate::{
//         parse::ParsableSegment,
//         passage_segments::{chapter_verse::ChapterVerse, chapter_verse_range::ChapterVerseRange},
//         segment::Segment,
//     };
//
//     use super::ChapterRange;
//
//     #[test]
//     fn try_from_passage_segment() -> Result<(), String> {
//         // "1:1" -> "1:1-1:1"
//         assert_eq!(
//             ChapterRange::try_from(Segment::ChapterVerse(ChapterVerse::parse_strict("1:1")?))?,
//             ChapterRange::parse_strict("1:1-1:1")?
//         );
//
//         // "1:1-2" -> "1:1-1:2"
//         assert_eq!(
//             ChapterRange::try_from(Segment::ChapterVerseRange(ChapterVerseRange::parse_strict(
//                 "1:1-2"
//             )?))?,
//             ChapterRange::parse_strict("1:1-1:2")?
//         );
//
//         // "1:2-3:4" -> "1:2-3:4"
//         assert_eq!(
//             ChapterRange::try_from(Segment::ChapterRange(ChapterRange::parse_strict(
//                 "1:2-3:4"
//             )?))?,
//             ChapterRange::parse_strict("1:2-3:4")?
//         );
//
//         Ok(())
//     }
//
//     #[test]
//     fn from_str() -> Result<(), String> {
//         assert_eq!(
//             ChapterRange::parse_strict("1:2-3:4")?,
//             ChapterRange::new(1, 2, 3, 4)
//         );
//         Ok(())
//     }
//
//     #[test]
//     fn to_string() {
//         assert_eq!(
//             ChapterRange::new(1, 2, 3, 4).to_string(),
//             String::from("1:2-3:4")
//         );
//     }
//
//     #[test]
//     fn serialize() -> Result<(), Box<dyn std::error::Error>> {
//         let map: BTreeMap<ChapterRange, u8> = BTreeMap::from([
//             (ChapterRange::new(1, 1, 1, 1), 0),
//             (ChapterRange::parse_strict("1:2-3:4")?, 1),
//         ]);
//
//         assert_eq!(
//             serde_json::to_value(&map)?,
//             json!({
//                 "1:1-1:1": 0,
//                 "1:2-3:4": 1
//             })
//         );
//
//         Ok(())
//     }
//
//     // #[test]
//     // #[should_panic]
//     // fn failed_serialize() {
//     //     serde_json::from_value::<BTreeMap<ChapterRange, u8>>(json!({
//     //         "1:1-1:1": 0,
//     //         "1:2-3": 1
//     //     })).unwrap();
//     // }
//
//     #[test]
//     fn deserialize() -> Result<(), Box<dyn std::error::Error>> {
//         let map: BTreeMap<ChapterRange, u8> = BTreeMap::from([
//             (ChapterRange::new(1, 1, 1, 1), 0),
//             (ChapterRange::parse_strict("1:2-3:4")?, 1),
//         ]);
//
//         assert_eq!(
//             serde_json::from_value::<BTreeMap<ChapterRange, u8>>(json!({
//                 "1:1-1:1": 0,
//                 "1:2-3:4": 1
//             }))?,
//             map
//         );
//
//         assert_eq!(
//             serde_json::from_value::<Vec<ChapterRange>>(json!(["1:2-3:4"]))?,
//             vec![ChapterRange::parse_strict("1:2-3:4")?]
//         );
//
//         Ok(())
//     }
//
//     // #[test]
//     // #[should_panic]
//     // fn failed_deserialize() {
//     //     serde_json::from_value::<BTreeMap<ChapterRange, u8>>(json!({
//     //         "1:1-1:1": 0,
//     //         "1:2-3": 1
//     //     })).unwrap();
//     // }
// }

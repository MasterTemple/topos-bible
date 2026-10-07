//! A file's references in a compact binary form, written and read by hand: an index loads
//! millions of them at startup, and this is several times faster than deriving their encoding
//!
//! Each reference is its book, its segments (a tag and one byte per number), then varints:
//! its start (as the distance from the previous one), length, line (as the change from the
//! previous one, zigzag), column, and for EPUBs its spine item and chapter.

use serde::{Deserializer, Serializer, de};
use topos_bible::{
    data::books::BookId,
    segments::{
        Passage, Segment, Segments,
        units::{
            chapter_range::ChapterRange, chapter_verse::ChapterVerse,
            chapter_verse_range::ChapterVerseRange, full_chapter::FullChapter,
            full_chapter_range::FullChapterRange, full_chapter_verse_range::FullChapterVerseRange,
        },
        verse_bounds::VerseBounds,
    },
};

use crate::entry::{Reference, Section};

fn put_varint(out: &mut Vec<u8>, mut n: u64) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

fn unzigzag(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

/// Encodes references (in any order, though in file order they're smallest)
pub fn encode(refs: &[Reference]) -> Vec<u8> {
    let mut out = Vec::with_capacity(refs.len() * 12);
    put_varint(&mut out, refs.len() as u64);
    let (mut start, mut line) = (0i64, 0i64);
    for r in refs {
        out.push(r.passage.book.0);
        out.push(r.passage.segments.len().min(255) as u8);
        for segment in r.passage.segments.iter().take(255) {
            match *segment {
                Segment::ChapterVerse(cv) => out.extend([0, cv.chapter, cv.verse]),
                Segment::ChapterVerseRange(r) => {
                    out.extend([1, r.chapter, r.verses.start, r.verses.end]);
                }
                Segment::ChapterRange(r) => out.extend([
                    2,
                    r.starting_chapter(),
                    r.starting_verse(),
                    r.ending_chapter(),
                    r.ending_verse().unwrap_or_default(),
                ]),
                Segment::FullChapter(f) => out.extend([3, f.chapter]),
                Segment::FullChapterRange(r) => {
                    out.extend([4, r.starting_chapter(), r.ending_chapter()]);
                }
                Segment::FullChapterVerseRange(r) => {
                    out.extend([5, r.start, r.end.chapter, r.end.verse]);
                }
            }
        }
        put_varint(&mut out, zigzag(i64::from(r.start) - start));
        start = i64::from(r.start);
        put_varint(&mut out, u64::from(r.len));
        put_varint(&mut out, zigzag(i64::from(r.line) - line));
        line = i64::from(r.line);
        put_varint(&mut out, u64::from(r.column));
        match r.section {
            None => out.push(0),
            Some(section) => {
                out.push(1);
                put_varint(&mut out, u64::from(section.spine));
                put_varint(&mut out, section.chapter.map_or(0, |c| u64::from(c) + 1));
            }
        }
    }
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn varint(&mut self) -> Option<u64> {
        let mut n = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.byte()?;
            n |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(n);
            }
        }
        None
    }

    fn u32(&mut self) -> Option<u32> {
        u32::try_from(self.varint()?).ok()
    }

    fn segment(&mut self) -> Option<Segment> {
        let mut b = || self.byte();
        Some(match b()? {
            0 => Segment::ChapterVerse(ChapterVerse::new(b()?, b()?)),
            1 => Segment::ChapterVerseRange(ChapterVerseRange::new(b()?, b()?, b()?)),
            2 => Segment::ChapterRange(ChapterRange::new(b()?, b()?, b()?, b()?)),
            3 => Segment::FullChapter(FullChapter::new(b()?)),
            4 => Segment::FullChapterRange(FullChapterRange::new(b()?, b()?)),
            5 => Segment::FullChapterVerseRange(FullChapterVerseRange::new(b()?, b()?, b()?)),
            _ => return None,
        })
    }
}

/// Decodes [`encode`]'s bytes; [`None`] if they're damaged
pub fn decode(bytes: &[u8]) -> Option<Vec<Reference>> {
    let mut r = Reader { bytes, at: 0 };
    let count = r.varint()? as usize;
    // Each reference takes at least 8 bytes, so a damaged count can't allocate much
    let mut refs = Vec::with_capacity(count.min(bytes.len() / 8 + 1));
    let (mut start, mut line) = (0i64, 0i64);
    for _ in 0..count {
        let book = BookId(r.byte()?);
        let n = r.byte()?;
        let mut segments = Segments::new();
        for _ in 0..n {
            segments.push(r.segment()?);
        }
        start += unzigzag(r.varint()?);
        let len = r.u32()?;
        line += unzigzag(r.varint()?);
        let column = r.u32()?;
        let section = match r.byte()? {
            0 => None,
            _ => {
                let spine = r.u32()?;
                let chapter = r.u32()?.checked_sub(1);
                Some(Section { spine, chapter })
            }
        };
        refs.push(Reference {
            passage: Passage { book, segments },
            start: u32::try_from(start).ok()?,
            len,
            line: u32::try_from(line).ok()?,
            column,
            section,
        });
    }
    (r.at == bytes.len()).then_some(refs)
}

/// For `#[serde(with)]`: references as one byte string
pub fn serialize<S: Serializer>(refs: &[Reference], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(&encode(refs))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Reference>, D::Error> {
    struct Bytes;
    impl<'de> de::Visitor<'de> for Bytes {
        type Value = Vec<Reference>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("encoded references")
        }

        fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
            decode(bytes).ok_or_else(|| E::custom("damaged references"))
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut bytes = vec![];
            while let Some(byte) = seq.next_element::<u8>()? {
                bytes.push(byte);
            }
            self.visit_bytes(&bytes)
        }
    }
    deserializer.deserialize_bytes(Bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use topos_bible::matcher::BibleMatcher;

    #[test]
    fn every_kind_of_segment_round_trips() {
        let text = "John 3:16, 1 Cor 13:4-7, Rom 8:28-9:2, Ps 23, Matt 5-7, John 1-2:3; Jude 5 \
                    and Gen 1:1; 3; 5:2-4";
        let matcher = BibleMatcher::default();
        let mut refs: Vec<Reference> = crate::entry::text_refs(&matcher, text, crate::Unit::Utf16);
        refs[1].section = Some(Section {
            spine: 7,
            chapter: None,
        });
        refs[2].section = Some(Section {
            spine: 300,
            chapter: Some(0),
        });
        // Out of order too (lines and starts going back)
        refs.swap(0, 3);
        let bytes = encode(&refs);
        assert_eq!(decode(&bytes), Some(refs.clone()));
        assert!(bytes.len() < refs.len() * 14, "{} bytes", bytes.len());
        // Damage is caught, not misread
        assert_eq!(decode(&bytes[..bytes.len() - 1]), None);
        let mut extra = bytes.clone();
        extra.push(0);
        assert_eq!(decode(&extra), None);
    }
}

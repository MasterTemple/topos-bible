//! [OSIS](https://www.crosswire.org/osis/) references (`John.3.16-John.3.18`) and integer keys.

use std::ops::RangeInclusive;

use crate::{
    data::books::{BookId, Books},
    error::{ToposError, ToposResult},
    segments::{passage::Passage, segment::Segment, verse_bounds::VerseBounds},
};

impl Passage {
    /**
    The passage as space-separated OSIS references
    - `John 3:16-18; 4` is `John.3.16-John.3.18 John.4`
    - [`None`] if the book has no OSIS id in the data
    */
    pub fn to_osis(&self, books: &Books) -> Option<String> {
        let book = books.get_osis(self.book)?;
        let refs: Vec<String> = self
            .segments
            .iter()
            .map(|seg| match seg {
                Segment::ChapterVerse(cv) => format!("{book}.{}.{}", cv.chapter, cv.verse),
                Segment::FullChapter(_) => format!("{book}.{}", seg.starting_chapter()),
                Segment::FullChapterRange(_) => format!(
                    "{book}.{}-{book}.{}",
                    seg.starting_chapter(),
                    seg.ending_chapter()
                ),
                Segment::FullChapterVerseRange(_) => format!(
                    "{book}.{}-{book}.{}.{}",
                    seg.starting_chapter(),
                    seg.ending_chapter(),
                    seg.ending_verse().unwrap_or_default()
                ),
                Segment::ChapterVerseRange(_) | Segment::ChapterRange(_) => format!(
                    "{book}.{}.{}-{book}.{}.{}",
                    seg.starting_chapter(),
                    seg.starting_verse(),
                    seg.ending_chapter(),
                    seg.ending_verse().unwrap_or_default()
                ),
            })
            .collect();
        Some(refs.join(" "))
    }

    /**
    One inclusive range of integer keys per segment, where a verse is
    `book * 1_000_000 + chapter * 1_000 + verse`
    - `John 3:16` is `43003016..=43003016`
    - A whole chapter ends at verse 999, so `John 3` is `43003001..=43003999`
    - Two passages overlap when any of their ranges overlap, which a database can index
    */
    pub fn bcv_ranges(&self) -> Vec<RangeInclusive<u32>> {
        let key = |chapter: u8, verse: u16| {
            u32::from(self.book.0) * 1_000_000 + u32::from(chapter) * 1_000 + u32::from(verse)
        };
        self.segments
            .iter()
            .map(|seg| {
                let start = key(seg.starting_chapter(), seg.starting_verse().into());
                let end_verse = seg.ending_verse().map_or(999, u16::from);
                start..=key(seg.ending_chapter(), end_verse)
            })
            .collect()
    }
}

impl Books {
    /// Parses space-separated OSIS references in one book (`John.3.16-John.3.18 John.4`)
    pub fn parse_osis(&self, input: &str) -> ToposResult<Passage> {
        let invalid = || ToposError::InvalidOsis(input.to_string());
        let mut book: Option<BookId> = None;
        let mut segments = vec![];
        for reference in input.split_whitespace() {
            let (start, end) = match reference.split_once('-') {
                Some((start, end)) => (start, Some(end)),
                None => (reference, None),
            };
            let (start_book, start) = self.parse_osis_point(start).ok_or_else(invalid)?;
            let end = match end {
                Some(end) => {
                    let (end_book, end) = self.parse_osis_point(end).ok_or_else(invalid)?;
                    if end_book != start_book {
                        return Err(invalid());
                    }
                    Some(end)
                }
                None => None,
            };
            if *book.get_or_insert(start_book) != start_book {
                return Err(invalid());
            }
            let segment = match (start, end) {
                ((c, None), None) => Segment::full_chapter(c),
                ((c, Some(v)), None) => Segment::chapter_verse(c, v),
                ((c, None), Some((c2, None))) => Segment::full_chapter_range(c, c2),
                ((c, Some(v)), Some((c2, Some(v2)))) => Segment::chapter_range(c, v, c2, v2),
                ((c, None), Some((c2, Some(v2)))) => Segment::chapter_range(c, 1, c2, v2),
                // `John.3.16-John.4` needs the length of chapter 4, which books do not know
                ((_, Some(_)), Some((_, None))) => return Err(invalid()),
            };
            segments.push(segment);
        }
        let book = book.ok_or_else(invalid)?;
        Ok(Passage {
            book,
            segments: crate::segments::passage::Segments(segments.into()),
        })
    }

    /// `John.3.16` as (book, (chapter, verse))
    fn parse_osis_point(&self, point: &str) -> Option<(BookId, (u8, Option<u8>))> {
        let mut parts = point.split('.');
        let book = self.search_osis(parts.next()?)?;
        let chapter = parts.next()?.parse().ok()?;
        let verse = parts.next().map(str::parse).transpose().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some((book, (chapter, verse)))
    }
}

#[cfg(test)]
mod tests {
    use crate::data::BibleData;

    #[test]
    fn round_trips() {
        let data = BibleData::default();
        let books = data.books();
        for (reference, osis) in [
            ("John 3:16", "John.3.16"),
            ("John 3:16-18", "John.3.16-John.3.18"),
            ("John 3:16-4:2", "John.3.16-John.4.2"),
            ("1 Samuel 3", "1Sam.3"),
            ("Romans 1-3", "Rom.1-Rom.3"),
            ("John 3:16, 18; 4", "John.3.16 John.3.18 John.4"),
        ] {
            let passage = books.parse(reference).unwrap();
            assert_eq!(passage.to_osis(books).unwrap(), osis, "{reference}");
            let parsed = books.parse_osis(osis).unwrap();
            assert_eq!(parsed.book, passage.book);
            assert_eq!(parsed.segments.0, passage.segments.0, "{osis}");
        }
    }

    #[test]
    fn rejects_invalid_osis() {
        let data = BibleData::default();
        for input in [
            "",
            "John",
            "Jn.3.16",
            "John.3.16-Rom.1.1",
            "John.3 Rom.1",
            "John.x",
        ] {
            assert!(data.books().parse_osis(input).is_err(), "{input}");
        }
    }

    #[test]
    fn bcv_keys() {
        let data = BibleData::default();
        let passage = data.books().parse("John 3:16, 3:18-4:2; 5").unwrap();
        assert_eq!(
            passage.bcv_ranges(),
            [
                43003016..=43003016,
                43003018..=43004002,
                43005001..=43005999
            ]
        );
    }
}

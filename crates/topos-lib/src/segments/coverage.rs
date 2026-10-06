//! Whether passages contain or overlap each other, verse by verse.

use std::ops::RangeInclusive;

use crate::{
    data::chapter_verses::ChapterVerses,
    segments::{passage::Passage, verse_bounds::VerseBounds},
};

/// Without versification, a chapter is treated as this many verses long
const UNKNOWN_CHAPTER_LENGTH: u32 = 999;

impl Passage {
    /**
    The verses as inclusive ranges of positions in the book, sorted and merged
    - With versification, positions count every verse from the start of the book, so
      `3:36` and `4:1` are adjacent and a whole chapter ends at its last verse
    - Without it, each chapter takes [`UNKNOWN_CHAPTER_LENGTH`] positions
    */
    pub fn verse_spans(&self, versification: Option<&ChapterVerses>) -> Vec<RangeInclusive<u32>> {
        let chapter_length = |chapter: u8| {
            versification
                .and_then(|v| v.get_last_verse(chapter))
                .map_or(UNKNOWN_CHAPTER_LENGTH, u32::from)
        };
        let position = |chapter: u8, verse: u32| {
            let before: u32 = (1..chapter).map(chapter_length).sum();
            before + verse
        };
        let mut spans: Vec<RangeInclusive<u32>> = self
            .segments
            .iter()
            .map(|seg| {
                let (start_chapter, end_chapter) = (seg.starting_chapter(), seg.ending_chapter());
                let end_verse = seg
                    .ending_verse()
                    .map_or(chapter_length(end_chapter), u32::from);
                position(start_chapter, seg.starting_verse().into())
                    ..=position(end_chapter, end_verse)
            })
            .collect();
        spans.sort_by_key(|span| *span.start());
        let mut merged: Vec<RangeInclusive<u32>> = vec![];
        for span in spans {
            match merged.last_mut() {
                Some(last) if *span.start() <= last.end() + 1 => {
                    *last = *last.start()..=*span.end().max(last.end());
                }
                _ => merged.push(span),
            }
        }
        merged
    }

    /// Whether every verse of `other` is in this passage
    pub fn contains_passage(&self, other: &Passage, versification: Option<&ChapterVerses>) -> bool {
        if self.book != other.book {
            return false;
        }
        let outer = self.verse_spans(versification);
        other.verse_spans(versification).iter().all(|inner| {
            outer
                .iter()
                .any(|span| span.start() <= inner.start() && inner.end() <= span.end())
        })
    }

    /// Whether any verse is in both passages
    pub fn overlaps_passage(&self, other: &Passage, versification: Option<&ChapterVerses>) -> bool {
        if self.book != other.book {
            return false;
        }
        let spans = self.verse_spans(versification);
        other.verse_spans(versification).iter().any(|other| {
            spans
                .iter()
                .any(|span| span.start() <= other.end() && other.start() <= span.end())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::data::{BibleData, books::BookId};

    fn check(outer: &str, inner: &str) -> (bool, bool) {
        let data = BibleData::default();
        let versification = data.chapter_verses().get_chapter_verses(&BookId(43));
        let outer = data.books().parse(&format!("John {outer}")).unwrap();
        let inner = data.books().parse(&format!("John {inner}")).unwrap();
        (
            outer.contains_passage(&inner, versification),
            outer.overlaps_passage(&inner, versification),
        )
    }

    #[test]
    fn containment_and_overlap() {
        // (contains, overlaps)
        assert_eq!(check("3", "3:16-18"), (true, true));
        assert_eq!(check("3:1-36", "3"), (true, true));
        assert_eq!(check("3:16-18", "3"), (false, true));
        assert_eq!(check("3:16", "3:17"), (false, false));
        // spans merge across chapters and segments
        assert_eq!(check("3:30-36; 4:1-2", "3:35-4:1"), (true, true));
        assert_eq!(check("3:1-5, 6-10", "3:4-7"), (true, true));
        assert_eq!(check("3-4", "4:54"), (true, true));
        assert_eq!(check("3:1-5", "3:5-6"), (false, true));
    }

    #[test]
    fn different_books_never_match() {
        let data = BibleData::default();
        let john = data.books().parse("John 3").unwrap();
        let romans = data.books().parse("Romans 3").unwrap();
        assert!(!john.contains_passage(&romans, None));
        assert!(!john.overlaps_passage(&romans, None));
    }
}

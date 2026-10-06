//! Writing passages as text, with configurable separators and styles (issue #4).

use crate::{
    data::bible_data::BibleData,
    segments::{passage::Passage, segment::Segment, verse_bounds::VerseBounds},
};

/// How the book is written
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BookStyle {
    /// `Genesis`
    #[default]
    Name,
    /// `Gn`
    Abbreviation,
    /// `Gen`
    Osis,
}

/**
How passages are written
- The defaults write the shortest form that parses back to the same passage
- Changing `chapter_separator` away from a `;` can make the output ambiguous (`3:16, 4` reads as
  verse 4)
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatOptions {
    pub book: BookStyle,
    /// Between the book and its segments (`" "`)
    pub book_separator: String,
    /// Between a chapter and a verse (`":"`)
    pub chapter_verse: String,
    /// Between the ends of a range (`"-"`)
    pub range: String,
    /// Before another verse in the same chapter (`","`)
    pub verse_separator: String,
    /// Before a segment in another chapter (`"; "`)
    pub chapter_separator: String,
    /// `1-2:3` instead of `1:1-2:3`
    pub omit_first_verse_of_chapter_range: bool,
    /// `3:16-18` instead of `3:16,17,18`
    pub join_adjacent: bool,
    /// `Jude 1:5` instead of `Jude 5`
    pub chapter_in_single_chapter_books: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            book: BookStyle::Name,
            book_separator: String::from(" "),
            chapter_verse: String::from(":"),
            range: String::from("-"),
            verse_separator: String::from(","),
            chapter_separator: String::from("; "),
            omit_first_verse_of_chapter_range: false,
            join_adjacent: false,
            chapter_in_single_chapter_books: true,
        }
    }
}

impl FormatOptions {
    /// `John 3:16-18; 4`, or [`None`] if the book is not in `data`
    pub fn passage(&self, passage: &Passage, data: &BibleData) -> Option<String> {
        let books = data.books();
        let book = match self.book {
            BookStyle::Name => books.get_name(passage.book),
            BookStyle::Abbreviation => books.get_abbrev(passage.book),
            BookStyle::Osis => books.get_osis(passage.book),
        }?;
        let single_chapter = data
            .chapter_verses()
            .get_chapter_verses(&passage.book)
            .is_some_and(|cv| cv.has_one_chapter());
        let segments = self.write(&passage.segments, single_chapter);
        Some(format!("{book}{}{segments}", self.book_separator))
    }

    /// `3:16-18; 4` (without book data, so single-chapter books keep their chapter)
    pub fn segments(&self, segments: &[Segment]) -> String {
        self.write(segments, false)
    }

    fn write(&self, segments: &[Segment], single_chapter: bool) -> String {
        let joined;
        let segments = if self.join_adjacent {
            joined = join_adjacent(segments);
            &joined
        } else {
            segments
        };
        let hide_chapter = single_chapter && !self.chapter_in_single_chapter_books;

        let mut out = String::new();
        let mut prev: Option<&Segment> = None;
        for seg in segments {
            match prev {
                None if hide_chapter => out.push_str(&self.verses_only(seg)),
                None => out.push_str(&self.full(seg)),
                Some(_) if hide_chapter => {
                    out.push_str(&self.verse_separator);
                    out.push_str(&self.verses_only(seg));
                }
                Some(prev) if continues(prev, seg) => {
                    out.push_str(&self.verse_separator);
                    out.push_str(&self.chapterless(seg));
                }
                Some(_) => {
                    out.push_str(&self.chapter_separator);
                    out.push_str(&self.full(seg));
                }
            }
            prev = Some(seg);
        }
        out
    }

    fn full(&self, seg: &Segment) -> String {
        let (cv, r) = (&self.chapter_verse, &self.range);
        let (sc, sv, ec) = (
            seg.starting_chapter(),
            seg.starting_verse(),
            seg.ending_chapter(),
        );
        let ev = seg.ending_verse().unwrap_or_default();
        match seg {
            Segment::ChapterVerse(_) => format!("{sc}{cv}{sv}"),
            Segment::ChapterVerseRange(_) => format!("{sc}{cv}{sv}{r}{ev}"),
            Segment::ChapterRange(_) if sv == 1 && self.omit_first_verse_of_chapter_range => {
                format!("{sc}{r}{ec}{cv}{ev}")
            }
            Segment::ChapterRange(_) => format!("{sc}{cv}{sv}{r}{ec}{cv}{ev}"),
            Segment::FullChapter(_) => format!("{sc}"),
            Segment::FullChapterRange(_) => format!("{sc}{r}{ec}"),
            Segment::FullChapterVerseRange(_) => format!("{sc}{r}{ec}{cv}{ev}"),
        }
    }

    /// Without the chapter it continues from (see [`continues`])
    fn chapterless(&self, seg: &Segment) -> String {
        let (cv, r) = (&self.chapter_verse, &self.range);
        let (sv, ec) = (seg.starting_verse(), seg.ending_chapter());
        let ev = seg.ending_verse().unwrap_or_default();
        match seg {
            Segment::ChapterVerse(_) => format!("{sv}"),
            Segment::ChapterVerseRange(_) => format!("{sv}{r}{ev}"),
            Segment::ChapterRange(_) => format!("{sv}{r}{ec}{cv}{ev}"),
            _ => self.full(seg),
        }
    }

    /// Verses in a single-chapter book (`5-7` for `1:5-7`)
    fn verses_only(&self, seg: &Segment) -> String {
        let (sv, r) = (seg.starting_verse(), &self.range);
        match (seg, seg.ending_verse()) {
            (Segment::ChapterVerse(_), _) => format!("{sv}"),
            (Segment::ChapterVerseRange(_), Some(ev)) => format!("{sv}{r}{ev}"),
            _ => self.full(seg),
        }
    }
}

/// Whether `seg` can be written without its chapter after `prev` and still parse the same way
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

/// Merges segments that continue each other (`3:16, 3:17` into `3:16-17`, `1, 2` into `1-2`)
fn join_adjacent(segments: &[Segment]) -> Vec<Segment> {
    let mut out: Vec<Segment> = vec![];
    for &seg in segments {
        let merged = out
            .last()
            .and_then(|prev| match (prev.ending_verse(), seg.ending_verse()) {
                (Some(end), Some(seg_end))
                    if seg.starting_chapter() == prev.ending_chapter()
                        && u16::from(seg.starting_verse()) == u16::from(end) + 1
                        && !matches!(seg, Segment::FullChapterVerseRange(_)) =>
                {
                    Some(Segment::chapter_range(
                        prev.starting_chapter(),
                        prev.starting_verse(),
                        seg.ending_chapter(),
                        seg_end,
                    ))
                }
                (None, None)
                    if u16::from(seg.starting_chapter())
                        == u16::from(prev.ending_chapter()) + 1 =>
                {
                    Some(Segment::full_chapter_range(
                        prev.starting_chapter(),
                        seg.ending_chapter(),
                    ))
                }
                _ => None,
            });
        match merged {
            Some(merged) => *out.last_mut().expect("merged with the last segment") = merged,
            None => out.push(seg),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::BibleData;

    fn format(options: FormatOptions, reference: &str) -> String {
        let data = BibleData::default();
        let passage = data.books().parse(reference).unwrap();
        options.passage(&passage, &data).unwrap()
    }

    #[test]
    fn defaults_round_trip() {
        let data = BibleData::default();
        for reference in [
            "John 3:16-18; 4",
            "John 5:1-3,5,7-9,12-6:6; 7:7-8:8",
            "Genesis 1,3",
        ] {
            let passage = data.books().parse(reference).unwrap();
            let written = FormatOptions::default().passage(&passage, &data).unwrap();
            assert_eq!(written, reference);
        }
    }

    #[test]
    fn styles_and_separators() {
        let options = FormatOptions {
            book: BookStyle::Osis,
            book_separator: String::from(" "),
            chapter_verse: String::from("."),
            range: String::from("–"),
            verse_separator: String::from(", "),
            ..FormatOptions::default()
        };
        assert_eq!(format(options, "1 Samuel 3:1-4, 7"), "1Sam 3.1–4, 7");
        let abbreviation = FormatOptions {
            book: BookStyle::Abbreviation,
            ..FormatOptions::default()
        };
        assert_eq!(format(abbreviation, "Genesis 1"), "Gn 1");
    }

    #[test]
    fn range_options() {
        let short = FormatOptions {
            omit_first_verse_of_chapter_range: true,
            ..FormatOptions::default()
        };
        assert_eq!(format(short, "John 1:1-2:3"), "John 1-2:3");

        let joined = FormatOptions {
            join_adjacent: true,
            ..FormatOptions::default()
        };
        assert_eq!(
            format(joined.clone(), "John 3:16, 17, 18, 20"),
            "John 3:16-18,20"
        );
        assert_eq!(format(joined.clone(), "John 3:36, 4:1"), "John 3:36; 4:1");
        assert_eq!(format(joined, "John 1, 2, 4"), "John 1-2,4");
    }

    #[test]
    fn single_chapter_books() {
        assert_eq!(format(FormatOptions::default(), "Jude 1:5-7"), "Jude 1:5-7");
        let bare = FormatOptions {
            chapter_in_single_chapter_books: false,
            ..FormatOptions::default()
        };
        assert_eq!(format(bare, "Jude 1:5-7, 9"), "Jude 5-7,9");
    }
}

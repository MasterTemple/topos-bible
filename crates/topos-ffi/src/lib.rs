//! The API that every language binding sees: owned data, one object, and no lifetimes.
//!
//! Offsets are reported in the unit the caller asks for, since languages index strings
//! differently: bytes (Rust, C), Unicode scalars (Python), or UTF-16 code units (JavaScript,
//! Kotlin, Swift's `utf16` view).

use boltffi::*;
use topos_lib::{
    data::{
        bible_data::{BibleData, BibleDataInput},
        books::BookId,
        chapter_verses::ChapterVerses,
    },
    filter::bible_filter::BibleFilter,
    matcher::BibleMatcher,
    segments::{
        Passage as CorePassage, Segment, Segments,
        autocomplete::{CompleteOptions, CompletionKind as CoreKind},
        formatter::{BookStyle as CoreBookStyle, FormatOptions},
        units::chapter_verse::ChapterVerse as CoreChapterVerse,
        verse_bounds::VerseBounds,
    },
};

/// How offsets and columns are counted
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffsetUnit {
    Byte,
    /// Unicode scalar values (Python string indices)
    Char,
    /// UTF-16 code units (JavaScript and Kotlin string indices)
    Utf16,
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookStyle {
    /// `Genesis`
    Name,
    /// `Gn`
    Abbreviation,
    /// `Gen`
    Osis,
}

/// A book in the data (for listing and filtering)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookInfo {
    /// 1 for Genesis through 66 for Revelation (with the default data)
    pub id: u8,
    pub name: String,
    pub abbreviation: String,
    pub osis: String,
    /// How many chapters it has (`0` if the data has no verse counts for it)
    pub chapters: u8,
}

/// A group of books, like `Gospels` or `Pauline Epistles`
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenreInfo {
    pub name: String,
    pub book_ids: Vec<u8>,
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChapterVerse {
    pub chapter: u8,
    pub verse: u8,
}

/// One part of a reference, as written
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassageSegment {
    /// `3:16` (no `end`), `3:16-18`, or `3:16-4:2`
    Verses {
        start: ChapterVerse,
        end: Option<ChapterVerse>,
    },
    /// `3` (no `end`) or `3-4`
    Chapters { start: u8, end: Option<u8> },
}

/// An inclusive range of verses with both ends written out (see [`Topos::verse_ranges`])
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerseRange {
    pub start: ChapterVerse,
    /// A whole chapter ends at its last verse; verse `0` only when a custom config has no verse
    /// counts for the book
    pub end: ChapterVerse,
}

#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passage {
    /// 1 for Genesis through 66 for Revelation (with the default data)
    pub book_id: u8,
    pub book: String,
    /// `John 3:16-18,20-4:2`
    pub reference: String,
    /// Each part of the reference as written: `John 3:16-18; 5` is verses 3:16 to 3:18 and
    /// chapter 5
    pub segments: Vec<PassageSegment>,
    /// `John.3.16-John.3.18 John.3.20-John.4.2`
    pub osis: String,
}

#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub passage: Passage,
    /// Start and end offsets in the searched text
    pub start: u32,
    pub end: u32,
    /// 1-based line and column of the start
    pub line: u32,
    pub column: u32,
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Book,
    Chapter,
    Verse,
}

#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// What a menu shows
    pub label: String,
    pub kind: CompletionKind,
    /// Replace the text from `start` to `end` with `text` to accept the completion
    pub start: u32,
    pub end: u32,
    pub text: String,
}

#[error]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToposError {
    /// The config JSON could not be read or used
    InvalidConfig { message: String },
}

/// Finds, parses, formats, and completes Bible references
pub struct Topos {
    matcher: BibleMatcher,
}

#[export]
impl Topos {
    /// With the default English book names and versification
    pub fn new() -> Self {
        Self {
            matcher: BibleMatcher::default(),
        }
    }

    /// With custom `books`, `genres`, or `chapter_verses` (see the CLI's `--config`)
    pub fn with_config(config_json: String) -> Result<Self, ToposError> {
        let invalid = |message: String| ToposError::InvalidConfig { message };
        let input: BibleDataInput =
            serde_json::from_str(&config_json).map_err(|e| invalid(e.to_string()))?;
        let data = BibleData::new(input).map_err(|e| invalid(e.to_string()))?;
        Ok(Self {
            matcher: BibleFilter::new(data).create_matcher(),
        })
    }

    /// Every reference in `text`
    pub fn search(&self, text: String, unit: OffsetUnit) -> Vec<Match> {
        let offsets = Offsets::new(&text, unit);
        self.matcher
            .search(&text)
            .into_iter()
            .filter_map(|m| {
                let bytes = m.location.bytes;
                let line_start = text[..bytes.start].rfind('\n').map_or(0, |i| i + 1);
                Some(Match {
                    passage: self.passage(&m.psg, CoreBookStyle::Name)?,
                    start: offsets.of_byte(bytes.start),
                    end: offsets.of_byte(bytes.end),
                    line: m.location.start.line as u32,
                    column: offsets.of_byte(bytes.start) - offsets.of_byte(line_start) + 1,
                })
            })
            .collect()
    }

    /// The first reference in `text`, read as one reference (`Jn 3:16` or `John.3.16`)
    pub fn parse(&self, reference: String, style: BookStyle) -> Option<Passage> {
        let books = self.matcher.data().books();
        let passage = books
            .parse(&reference)
            .or_else(|| books.parse_osis(&reference).ok())?;
        self.passage(&passage, style.into())
    }

    /// Every book, in order
    pub fn books(&self) -> Vec<BookInfo> {
        let data = self.matcher.data();
        let books = data.books();
        books
            .ids()
            .map(|id| BookInfo {
                id: id.0,
                name: books.get_name(id).cloned().unwrap_or_default(),
                abbreviation: books.get_abbrev(id).cloned().unwrap_or_default(),
                osis: books.get_osis(id).cloned().unwrap_or_default(),
                chapters: data
                    .chapter_verses()
                    .get_chapter_verses(&id)
                    .map_or(0, |cv| cv.get_chapter_count()),
            })
            .collect()
    }

    /// Every genre (book group), in order
    pub fn genres(&self) -> Vec<GenreInfo> {
        self.matcher
            .data()
            .genres()
            .iter()
            .map(|genre| GenreInfo {
                name: genre.name().to_string(),
                book_ids: genre.books().iter().map(|id| id.0).collect(),
            })
            .collect()
    }

    /// The book with this name or abbreviation (`Jn`, `1 Cor`), ignoring case
    pub fn find_book(&self, name: String) -> Option<u8> {
        self.matcher.data().books().search(&name).map(|id| id.0)
    }

    /// The genre with this name or abbreviation (`gospels`, `pauline`), ignoring case
    pub fn find_genre(&self, name: String) -> Option<GenreInfo> {
        let genre = self.matcher.data().genres().get(&name)?;
        Some(GenreInfo {
            name: genre.name().to_string(),
            book_ids: genre.books().iter().map(|id| id.0).collect(),
        })
    }

    /// Each segment as an explicit verse range: whole chapters run from verse 1 to their last verse
    pub fn verse_ranges(&self, passage: Passage) -> Vec<VerseRange> {
        let passage = CorePassage::from(&passage);
        passage
            .ranges(self.versification(&passage))
            .into_iter()
            .map(|r| VerseRange {
                start: r.start.into(),
                end: r.end.into(),
            })
            .collect()
    }

    /// Every verse in the passage, in order (`John 3:16-18` is 3:16, 3:17, and 3:18)
    pub fn verses(&self, passage: Passage) -> Vec<ChapterVerse> {
        let passage = CorePassage::from(&passage);
        passage
            .verses(self.versification(&passage))
            .into_iter()
            .map(ChapterVerse::from)
            .collect()
    }

    /// Whether every verse of `inner` is in `outer`
    pub fn contains(&self, outer: Passage, inner: Passage) -> bool {
        let (outer, inner) = (CorePassage::from(&outer), CorePassage::from(&inner));
        outer.contains_passage(&inner, self.versification(&outer))
    }

    /// Whether the passages share any verse
    pub fn overlaps(&self, a: Passage, b: Passage) -> bool {
        let (a, b) = (CorePassage::from(&a), CorePassage::from(&b));
        a.overlaps_passage(&b, self.versification(&a))
    }

    /// Completions for the reference that ends at `cursor`
    pub fn complete(
        &self,
        text: String,
        cursor: u32,
        unit: OffsetUnit,
        style: BookStyle,
        limit: u32,
    ) -> Vec<Completion> {
        let offsets = Offsets::new(&text, unit);
        let Some(cursor) = offsets.byte_of(cursor) else {
            return vec![];
        };
        let options = CompleteOptions {
            format: FormatOptions {
                book: style.into(),
                ..FormatOptions::default()
            },
            limit: (limit > 0).then_some(limit as usize),
        };
        self.matcher
            .complete(&text, cursor, &options)
            .into_iter()
            .map(|c| Completion {
                label: c.label,
                kind: match c.kind {
                    CoreKind::Book => CompletionKind::Book,
                    CoreKind::Chapter => CompletionKind::Chapter,
                    CoreKind::Verse => CompletionKind::Verse,
                },
                start: offsets.of_byte(c.edit.range.start),
                end: offsets.of_byte(c.edit.range.end),
                text: c.edit.text,
            })
            .collect()
    }
}

impl Default for Topos {
    fn default() -> Self {
        Self::new()
    }
}

impl Topos {
    fn versification(&self, passage: &CorePassage) -> Option<&ChapterVerses> {
        self.matcher
            .data()
            .chapter_verses()
            .get_chapter_verses(&passage.book)
    }

    fn passage(&self, passage: &CorePassage, style: CoreBookStyle) -> Option<Passage> {
        let data = self.matcher.data();
        let options = FormatOptions {
            book: style,
            ..FormatOptions::default()
        };
        let segments = passage.segments.iter().map(PassageSegment::from).collect();
        Some(Passage {
            book_id: passage.book.0,
            book: data.books().get_name(passage.book)?.clone(),
            reference: options.passage(passage, data)?,
            segments,
            osis: passage.to_osis(data.books()).unwrap_or_default(),
        })
    }
}

impl From<CoreChapterVerse> for ChapterVerse {
    fn from(cv: CoreChapterVerse) -> Self {
        Self {
            chapter: cv.chapter,
            verse: cv.verse,
        }
    }
}

impl From<&Segment> for PassageSegment {
    fn from(segment: &Segment) -> Self {
        let cv = |chapter, verse| ChapterVerse { chapter, verse };
        let (sc, sv, ec) = (
            segment.starting_chapter(),
            segment.starting_verse(),
            segment.ending_chapter(),
        );
        match (segment, segment.ending_verse()) {
            (Segment::ChapterVerse(_), _) => Self::Verses {
                start: cv(sc, sv),
                end: None,
            },
            (Segment::FullChapter(_), _) => Self::Chapters {
                start: sc,
                end: None,
            },
            (Segment::FullChapterRange(_), _) => Self::Chapters {
                start: sc,
                end: Some(ec),
            },
            // Verse ranges, including `1-2:3` (from 1:1)
            (_, end_verse) => Self::Verses {
                start: cv(sc, sv),
                end: Some(cv(ec, end_verse.unwrap_or_default())),
            },
        }
    }
}

/// Back to the core types, for the helpers that take a passage
impl From<&Passage> for CorePassage {
    fn from(passage: &Passage) -> Self {
        let segments = passage
            .segments
            .iter()
            .map(|segment| match *segment {
                PassageSegment::Verses { start, end: None } => {
                    Segment::chapter_verse(start.chapter, start.verse)
                }
                PassageSegment::Verses {
                    start,
                    end: Some(end),
                } => Segment::chapter_range(start.chapter, start.verse, end.chapter, end.verse),
                PassageSegment::Chapters { start, end: None } => Segment::full_chapter(start),
                PassageSegment::Chapters {
                    start,
                    end: Some(end),
                } => Segment::full_chapter_range(start, end),
            })
            .collect();
        Segments(segments).with_book(BookId(passage.book_id))
    }
}

impl From<BookStyle> for CoreBookStyle {
    fn from(style: BookStyle) -> Self {
        match style {
            BookStyle::Name => CoreBookStyle::Name,
            BookStyle::Abbreviation => CoreBookStyle::Abbreviation,
            BookStyle::Osis => CoreBookStyle::Osis,
        }
    }
}

/// Converts between byte offsets and the caller's unit
struct Offsets<'a> {
    text: &'a str,
    unit: OffsetUnit,
}

impl<'a> Offsets<'a> {
    fn new(text: &'a str, unit: OffsetUnit) -> Self {
        Self { text, unit }
    }

    fn of_byte(&self, byte: usize) -> u32 {
        let before = &self.text[..byte];
        let offset = match self.unit {
            OffsetUnit::Byte => byte,
            OffsetUnit::Char => before.chars().count(),
            OffsetUnit::Utf16 => before.encode_utf16().count(),
        };
        offset as u32
    }

    /// [`None`] if the offset is past the end or inside a character
    fn byte_of(&self, offset: u32) -> Option<usize> {
        let offset = offset as usize;
        match self.unit {
            OffsetUnit::Byte => self.text.is_char_boundary(offset).then_some(offset),
            OffsetUnit::Char => self
                .text
                .char_indices()
                .map(|(idx, _)| idx)
                .chain(std::iter::once(self.text.len()))
                .nth(offset),
            OffsetUnit::Utf16 => {
                let mut units = 0;
                for (idx, c) in self.text.char_indices() {
                    if units == offset {
                        return Some(idx);
                    }
                    units += c.len_utf16();
                }
                (units == offset).then_some(self.text.len())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_reports_offsets_in_the_callers_unit() {
        let topos = Topos::new();
        // `é` is 2 bytes and 1 UTF-16 unit; `𝄞` is 4 bytes and 2 UTF-16 units
        let text = String::from("é𝄞 John 3:16");
        let byte = &topos.search(text.clone(), OffsetUnit::Byte)[0];
        let utf16 = &topos.search(text.clone(), OffsetUnit::Utf16)[0];
        let char = &topos.search(text, OffsetUnit::Char)[0];
        assert_eq!((byte.start, byte.end), (7, 16));
        assert_eq!((utf16.start, utf16.end), (4, 13));
        assert_eq!((char.start, char.end, char.column), (3, 12, 4));
        assert_eq!(byte.passage.osis, "John.3.16");
    }

    #[test]
    fn parse_and_complete() {
        let topos = Topos::new();
        let passage = topos
            .parse("Jn 3:16-18".into(), BookStyle::Abbreviation)
            .unwrap();
        assert_eq!(passage.reference, "Jn 3:16-18");
        assert_eq!(
            topos
                .parse("John.3.16".into(), BookStyle::Name)
                .unwrap()
                .reference,
            "John 3:16"
        );

        let text = String::from("日 Gen 1:");
        let completions = topos.complete(text, 8, OffsetUnit::Utf16, BookStyle::Name, 2);
        assert_eq!(completions.len(), 2);
        assert_eq!((completions[0].start, completions[0].end), (2, 8));
        assert_eq!(completions[0].text, "Genesis 1:1");
    }

    fn cv(chapter: u8, verse: u8) -> ChapterVerse {
        ChapterVerse { chapter, verse }
    }

    #[test]
    fn passages_list_segments_as_written() {
        let topos = Topos::new();
        let found = topos.search(
            "Read John 3:16-18,20-4:2; 5-6; 7; 8:1".into(),
            OffsetUnit::Utf16,
        );
        let passage = &found[0].passage;
        assert_eq!(passage.reference, "John 3:16-18,20-4:2; 5-6,7; 8:1");
        assert_eq!(
            passage.segments,
            [
                PassageSegment::Verses {
                    start: cv(3, 16),
                    end: Some(cv(3, 18))
                },
                PassageSegment::Verses {
                    start: cv(3, 20),
                    end: Some(cv(4, 2))
                },
                PassageSegment::Chapters {
                    start: 5,
                    end: Some(6)
                },
                PassageSegment::Chapters {
                    start: 7,
                    end: None
                },
                PassageSegment::Verses {
                    start: cv(8, 1),
                    end: None
                },
            ]
        );
    }

    #[test]
    fn helpers() {
        let topos = Topos::new();
        let parse = |reference: &str| topos.parse(reference.into(), BookStyle::Name).unwrap();

        let ranges = topos.verse_ranges(parse("John 3; 4:1-2"));
        assert_eq!(
            ranges,
            [
                VerseRange {
                    start: cv(3, 1),
                    end: cv(3, 36)
                },
                VerseRange {
                    start: cv(4, 1),
                    end: cv(4, 2)
                },
            ]
        );
        assert_eq!(
            topos.verses(parse("John 3:35-4:1")),
            [cv(3, 35), cv(3, 36), cv(4, 1)]
        );
        assert_eq!(topos.verses(parse("Jude 1")).len(), 25);

        assert!(topos.contains(parse("John 3"), parse("John 3:16-18")));
        assert!(!topos.contains(parse("John 3:16"), parse("John 3:16-17")));
        assert!(topos.overlaps(parse("John 3:16-17"), parse("John 3:17-4:1")));
        assert!(!topos.overlaps(parse("John 3"), parse("Romans 3")));
    }

    #[test]
    fn books_and_genres() {
        let topos = Topos::new();
        let books = topos.books();
        assert_eq!(books.len(), 66);
        assert_eq!(
            books[42],
            BookInfo {
                id: 43,
                name: "John".into(),
                abbreviation: "Jn".into(),
                osis: "John".into(),
                chapters: 21
            }
        );
        assert_eq!(topos.find_book("1 cor".into()), Some(46));
        assert_eq!(topos.find_book("nope".into()), None);

        let genres = topos.genres();
        let gospels = genres.iter().find(|g| g.name == "Gospels").unwrap();
        assert_eq!(gospels.book_ids, [40, 41, 42, 43]);
        // Genres made of other genres include their books
        let prophets = genres.iter().find(|g| g.name == "Prophets").unwrap();
        assert_eq!(prophets.book_ids.len(), 17);
        assert_eq!(topos.find_genre("gospels".into()).unwrap().name, "Gospels");
    }

    #[test]
    fn custom_config() {
        let config = r#"{"books": [{"id": 1, "book": "Bereshit", "abbreviation": "Ber"}]}"#;
        let topos = Topos::with_config(config.into()).unwrap();
        assert_eq!(topos.search("Bereshit 1".into(), OffsetUnit::Byte).len(), 1);
        assert!(Topos::with_config("{".into()).is_err());
    }
}

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
        patch::DataPatch,
    },
    filter::{
        bible_filter::BibleFilter,
        filters::{book::BookFilter, genre::GenreFilter, testament::TestamentFilter},
    },
    matcher::{BibleMatcher, context::BookContext},
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
    /// A query names a book, genre, or passage that doesn't exist
    InvalidQuery { message: String },
}

/// A testament, for [`ToposQuery`]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Testament {
    Old,
    New,
}

/**
Which references to keep, like the CLI's filter options. Build one by chaining: each method returns
a new query, so a query can be reused and extended

```ts
const query = ToposQuery.create().newTestament().genre("Pauline Epistles").explicitOverlap("Romans 8");
topos.searchWith(text, OffsetUnit.Utf16, query);
```

- Including a testament limits the search to it; included genres and books add up within it;
  exclusions always win
- The passage filters that keep references (inside, any, explicit, exact overlap) are joined with
  OR; `excludeOverlap` then drops references
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToposQuery {
    pub testaments: Vec<Testament>,
    pub exclude_testaments: Vec<Testament>,
    /// Genre names or abbreviations (`Gospels`, `pauline`)
    pub genres: Vec<String>,
    pub exclude_genres: Vec<String>,
    /// Book names or abbreviations (`John`, `1 Cor`)
    pub books: Vec<String>,
    pub exclude_books: Vec<String>,
    /// Keep references entirely inside one of these passages (`-i`)
    pub inside: Vec<String>,
    /// Keep references that share any verse with one of these (`--any-overlap`)
    pub any_overlap: Vec<String>,
    /// Keep references that name a verse of one of these; whole chapters don't count (`-o`)
    pub explicit_overlap: Vec<String>,
    /// Keep references that are exactly one of these passages (`--exact-overlap`)
    pub exact_overlap: Vec<String>,
    /// Drop references that share any verse with one of these (`--exclude-overlap`)
    pub exclude_overlap: Vec<String>,
}

fn with<T>(mut list: Vec<T>, value: T) -> Vec<T> {
    list.push(value);
    list
}

#[export]
impl ToposQuery {
    /// A query that keeps every reference
    pub fn create() -> Self {
        Self::default()
    }

    pub fn testament(&self, testament: Testament) -> Self {
        Self {
            testaments: with(self.testaments.clone(), testament),
            ..self.clone()
        }
    }

    /// Only the Old Testament (`--ot`)
    pub fn old_testament(&self) -> Self {
        self.testament(Testament::Old)
    }

    /// Only the New Testament (`--nt`)
    pub fn new_testament(&self) -> Self {
        self.testament(Testament::New)
    }

    pub fn exclude_testament(&self, testament: Testament) -> Self {
        Self {
            exclude_testaments: with(self.exclude_testaments.clone(), testament),
            ..self.clone()
        }
    }

    pub fn genre(&self, genre: String) -> Self {
        Self {
            genres: with(self.genres.clone(), genre),
            ..self.clone()
        }
    }

    pub fn exclude_genre(&self, genre: String) -> Self {
        Self {
            exclude_genres: with(self.exclude_genres.clone(), genre),
            ..self.clone()
        }
    }

    pub fn book(&self, book: String) -> Self {
        Self {
            books: with(self.books.clone(), book),
            ..self.clone()
        }
    }

    pub fn exclude_book(&self, book: String) -> Self {
        Self {
            exclude_books: with(self.exclude_books.clone(), book),
            ..self.clone()
        }
    }

    pub fn inside(&self, passage: String) -> Self {
        Self {
            inside: with(self.inside.clone(), passage),
            ..self.clone()
        }
    }

    pub fn any_overlap(&self, passage: String) -> Self {
        Self {
            any_overlap: with(self.any_overlap.clone(), passage),
            ..self.clone()
        }
    }

    pub fn explicit_overlap(&self, passage: String) -> Self {
        Self {
            explicit_overlap: with(self.explicit_overlap.clone(), passage),
            ..self.clone()
        }
    }

    pub fn exact_overlap(&self, passage: String) -> Self {
        Self {
            exact_overlap: with(self.exact_overlap.clone(), passage),
            ..self.clone()
        }
    }

    pub fn exclude_overlap(&self, passage: String) -> Self {
        Self {
            exclude_overlap: with(self.exclude_overlap.clone(), passage),
            ..self.clone()
        }
    }
}

/**
How references are written, like the CLI's `--psg-fmt` and `--fmt-*` options. Chain the methods
(each returns a new format), or start from the CLI's JSON with `withJson`

```ts
const format = ToposFormat.create().book(BookStyle.Abbreviation).joinAdjacent(true).range("\u2013");
topos.formatPassage(passage, format); // "Jn 3:16–18"
```

The defaults write `John 3:16,17,18; 4` (`Jude 1:5` keeps its chapter)
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToposFormat {
    options: FormatOptions,
}

#[export]
impl ToposFormat {
    /// The default format
    pub fn create() -> Self {
        Self::default()
    }

    /// This format with the fields in a JSON object (the CLI's `--psg-fmt`), like
    /// `{"join_adjacent": true, "chapter_verse": "."}`; unknown fields are an error
    pub fn with_json(&self, json: String) -> Result<ToposFormat, ToposError> {
        let invalid = |e: serde_json::Error| ToposError::InvalidConfig {
            message: e.to_string(),
        };
        // Start from this format's fields, then apply the JSON's
        let mut fields = serde_json::to_value(&self.options).map_err(invalid)?;
        let changes: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&json).map_err(invalid)?;
        if let Some(object) = fields.as_object_mut() {
            object.extend(changes);
        }
        let options = serde_json::from_value(fields).map_err(invalid)?;
        Ok(Self { options })
    }

    pub fn book(&self, style: BookStyle) -> Self {
        self.with(|o| o.book = style.into())
    }

    /// Between the book and its chapters (`" "`)
    pub fn book_separator(&self, separator: String) -> Self {
        self.with(|o| o.book_separator = separator)
    }

    /// Between a chapter and a verse (`":"`)
    pub fn chapter_verse(&self, separator: String) -> Self {
        self.with(|o| o.chapter_verse = separator)
    }

    /// Between the ends of a range (`"-"`)
    pub fn range(&self, separator: String) -> Self {
        self.with(|o| o.range = separator)
    }

    /// Before another verse in the same chapter (`","`)
    pub fn verse_separator(&self, separator: String) -> Self {
        self.with(|o| o.verse_separator = separator)
    }

    /// Before a part in another chapter (`"; "`)
    pub fn chapter_separator(&self, separator: String) -> Self {
        self.with(|o| o.chapter_separator = separator)
    }

    /// `3:16-18` instead of `3:16,17,18`
    pub fn join_adjacent(&self, join: bool) -> Self {
        self.with(|o| o.join_adjacent = join)
    }

    /// `1-2:3` instead of `1:1-2:3`
    pub fn omit_first_verse_of_chapter_range(&self, omit: bool) -> Self {
        self.with(|o| o.omit_first_verse_of_chapter_range = omit)
    }

    /// `Jude 1:5` (true, the default) or `Jude 5`
    pub fn chapter_in_single_chapter_books(&self, chapter: bool) -> Self {
        self.with(|o| o.chapter_in_single_chapter_books = chapter)
    }
}

impl ToposFormat {
    fn with(&self, change: impl FnOnce(&mut FormatOptions)) -> Self {
        let mut options = self.options.clone();
        change(&mut options);
        Self { options }
    }
}

/**
How to build a [`Topos`]: its data and book context, like the CLI's `--data`, `--merge-data`,
`--remove-data`, `--context-book`, and `--context-heading`. Chain the methods, then pass it to
[`ToposOptions::build`]

```ts
const options = ToposOptions.create().mergeData('{"books":[{"book":"John","abbreviations":["jhn"]}]}');
const topos = options.build();
```
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToposOptions {
    /// Custom data JSON that replaces the defaults (each part it has)
    pub data: Option<String>,
    /// JSON patches merged into the data, in order (names and values are added once each)
    pub merge_data: Vec<String>,
    /// JSON patches removed from the data, after merging (a book or genre listed alone is removed
    /// entirely)
    pub remove_data: Vec<String>,
    /// The book that references without one (`3:16`) belong to
    pub context_book: Option<String>,
    /// A pattern for headings that set the book for what follows, like `^#+ {book}$`
    pub context_heading: Option<String>,
    /// How references in results are written
    pub format: FormatOptions,
}

#[export]
impl ToposOptions {
    /// The defaults: English book names and versification, no book context
    pub fn create() -> Self {
        Self::default()
    }

    pub fn data(&self, json: String) -> Self {
        Self {
            data: Some(json),
            ..self.clone()
        }
    }

    pub fn merge_data(&self, json: String) -> Self {
        Self {
            merge_data: with(self.merge_data.clone(), json),
            ..self.clone()
        }
    }

    pub fn remove_data(&self, json: String) -> Self {
        Self {
            remove_data: with(self.remove_data.clone(), json),
            ..self.clone()
        }
    }

    pub fn context_book(&self, book: String) -> Self {
        Self {
            context_book: Some(book),
            ..self.clone()
        }
    }

    pub fn context_heading(&self, pattern: String) -> Self {
        Self {
            context_heading: Some(pattern),
            ..self.clone()
        }
    }

    /// How references in `search` and `searchWith` results are written (`parse` and `complete`
    /// use it too, with the book style they are given)
    pub fn format(&self, format: &ToposFormat) -> Self {
        Self {
            format: format.options.clone(),
            ..self.clone()
        }
    }

    /// A [`Topos`] with these options (an error if the data or context can't be used)
    pub fn build(&self) -> Result<Topos, ToposError> {
        Topos::from_options(self)
    }
}

/// Finds, parses, formats, and completes Bible references
pub struct Topos {
    matcher: BibleMatcher,
    /// How references in results are written (see [`ToposOptions::format`])
    format: FormatOptions,
}

#[export]
impl Topos {
    /// With the default English book names and versification
    pub fn new() -> Self {
        Self {
            matcher: BibleMatcher::default(),
            format: FormatOptions::default(),
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
            format: FormatOptions::default(),
        })
    }

    /// Every reference in `text`
    pub fn search(&self, text: String, unit: OffsetUnit) -> Vec<Match> {
        self.matches(&self.matcher, &text, unit)
    }

    /// The references in `text` that the query keeps
    pub fn search_with(
        &self,
        text: String,
        unit: OffsetUnit,
        query: &ToposQuery,
    ) -> Result<Vec<Match>, ToposError> {
        let matcher = self.filter(query)?.create_matcher();
        let matcher = match self.matcher.context() {
            Some(context) => matcher.with_context(context.clone()),
            None => matcher,
        };
        Ok(self.matches(&matcher, &text, unit))
    }

    /// Why nothing can match the query, if its filters contradict each other (like the Old
    /// Testament and the Pauline Epistles)
    pub fn contradiction(&self, query: &ToposQuery) -> Result<Option<String>, ToposError> {
        Ok(self.filter(query)?.contradiction())
    }

    /// The first reference in `text`, read as one reference (`Jn 3:16` or `John.3.16`)
    pub fn parse(&self, reference: String, style: BookStyle) -> Option<Passage> {
        let books = self.matcher.data().books();
        // OSIS first: as plain text, `John.3.16-John.3.18` would read as just John 3:16
        let passage = books
            .parse_osis(&reference)
            .ok()
            .or_else(|| books.parse(&reference))?;
        self.passage(&passage, &self.format_with(style))
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
        self.completions(text, cursor, unit, self.format_with(style), limit)
    }

    /// Like `complete`, writing completions with a [`ToposFormat`] (separators, joined ranges,
    /// and the book style)
    pub fn complete_with(
        &self,
        text: String,
        cursor: u32,
        unit: OffsetUnit,
        format: &ToposFormat,
        limit: u32,
    ) -> Vec<Completion> {
        self.completions(text, cursor, unit, format.options.clone(), limit)
    }

    /// A passage written with a [`ToposFormat`] (`None` if its book isn't in this instance's data)
    pub fn format_passage(&self, passage: Passage, format: &ToposFormat) -> Option<String> {
        format
            .options
            .passage(&CorePassage::from(&passage), self.matcher.data())
    }
}

impl Topos {
    fn completions(
        &self,
        text: String,
        cursor: u32,
        unit: OffsetUnit,
        format: FormatOptions,
        limit: u32,
    ) -> Vec<Completion> {
        let offsets = Offsets::new(&text, unit);
        let Some(cursor) = offsets.byte_of(cursor) else {
            return vec![];
        };
        let options = CompleteOptions {
            format,
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
    /// Builds an instance from [`ToposOptions`]
    fn from_options(options: &ToposOptions) -> Result<Self, ToposError> {
        let invalid = |message: String| ToposError::InvalidConfig { message };
        fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, ToposError> {
            serde_json::from_str(json).map_err(|e| ToposError::InvalidConfig {
                message: e.to_string(),
            })
        }
        let mut input = match &options.data {
            Some(json) => parse(json)?,
            None => BibleDataInput::defaults(),
        };
        for json in &options.merge_data {
            input
                .merge(parse::<DataPatch>(json)?)
                .map_err(|e| invalid(e.to_string()))?;
        }
        for json in &options.remove_data {
            input
                .remove(parse::<DataPatch>(json)?)
                .map_err(|e| invalid(e.to_string()))?;
        }
        let data = BibleData::new(input).map_err(|e| invalid(e.to_string()))?;
        let context = match (&options.context_book, &options.context_heading) {
            (Some(book), _) => {
                let id = data
                    .books()
                    .search(book)
                    .ok_or_else(|| invalid(format!("unknown book `{book}`")))?;
                Some(BookContext::Book(id))
            }
            (None, Some(pattern)) => Some(
                BookContext::headings(data.books(), pattern).map_err(|e| invalid(e.to_string()))?,
            ),
            (None, None) => None,
        };
        let matcher = BibleFilter::new(data).create_matcher();
        Ok(Self {
            matcher: match &context {
                Some(context) => matcher.with_context(context.clone()),
                None => matcher,
            },
            format: options.format.clone(),
        })
    }

    fn matches(&self, matcher: &BibleMatcher, text: &str, unit: OffsetUnit) -> Vec<Match> {
        let offsets = Offsets::new(text, unit);
        matcher
            .search(text)
            .into_iter()
            .filter_map(|m| {
                let bytes = m.location.bytes;
                let line_start = text[..bytes.start].rfind('\n').map_or(0, |i| i + 1);
                Some(Match {
                    passage: self.passage(&m.psg, &self.format)?,
                    start: offsets.of_byte(bytes.start),
                    end: offsets.of_byte(bytes.end),
                    line: m.location.start.line as u32,
                    column: offsets.of_byte(bytes.start) - offsets.of_byte(line_start) + 1,
                })
            })
            .collect()
    }

    /// The query as a filter over this instance's data
    fn filter(&self, query: &ToposQuery) -> Result<BibleFilter, ToposError> {
        let invalid = |e: topos_lib::error::ToposError| ToposError::InvalidQuery {
            message: e.to_string(),
        };
        let testament = |t: &Testament| match t {
            Testament::Old => TestamentFilter::Old,
            Testament::New => TestamentFilter::New,
        };
        let mut filter = BibleFilter::new(self.matcher.data().clone());
        filter
            .include_many(query.testaments.iter().map(testament))
            .map_err(invalid)?;
        filter
            .include_many(query.genres.iter().map(GenreFilter::new))
            .map_err(invalid)?;
        filter
            .include_many(query.books.iter().map(BookFilter::new))
            .map_err(invalid)?;
        filter
            .exclude_many(query.exclude_testaments.iter().map(testament))
            .map_err(invalid)?;
        filter
            .exclude_many(query.exclude_genres.iter().map(GenreFilter::new))
            .map_err(invalid)?;
        filter
            .exclude_many(query.exclude_books.iter().map(BookFilter::new))
            .map_err(invalid)?;
        for passage in &query.inside {
            filter.filter_inside(passage).map_err(invalid)?;
        }
        for passage in &query.any_overlap {
            filter.filter_any_overlap(passage).map_err(invalid)?;
        }
        for passage in &query.explicit_overlap {
            filter.filter_explicit_overlap(passage).map_err(invalid)?;
        }
        for passage in &query.exact_overlap {
            filter.filter_exact_overlap(passage).map_err(invalid)?;
        }
        for passage in &query.exclude_overlap {
            filter.filter_exclude_overlap(passage).map_err(invalid)?;
        }
        Ok(filter)
    }

    fn versification(&self, passage: &CorePassage) -> Option<&ChapterVerses> {
        self.matcher
            .data()
            .chapter_verses()
            .get_chapter_verses(&passage.book)
    }

    /// The instance's format with another book style
    fn format_with(&self, style: BookStyle) -> FormatOptions {
        FormatOptions {
            book: style.into(),
            ..self.format.clone()
        }
    }

    fn passage(&self, passage: &CorePassage, options: &FormatOptions) -> Option<Passage> {
        let data = self.matcher.data();
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

    fn references(matches: Vec<Match>) -> Vec<String> {
        matches.into_iter().map(|m| m.passage.reference).collect()
    }

    #[test]
    fn queries_chain_and_filter() {
        let topos = Topos::new();
        let text = "John 3, John 3:14-18, John 2; 3:16, Rom 8:28, Gen 1:1".to_string();
        let search = |query: &ToposQuery| {
            references(
                topos
                    .search_with(text.clone(), OffsetUnit::Byte, query)
                    .unwrap(),
            )
        };
        let nt = ToposQuery::create().new_testament();
        assert_eq!(
            search(&nt.explicit_overlap("John 3:16".into())),
            ["John 3:14-18", "John 2; 3:16"]
        );
        // Each method returns a new query, so `nt` is unchanged
        assert_eq!(search(&nt).len(), 4);
        assert_eq!(search(&nt.any_overlap("John 3:16".into())).len(), 3);
        assert_eq!(
            search(&ToposQuery::create().exact_overlap("Rom 8:28".into())),
            ["Romans 8:28"]
        );
        assert_eq!(
            search(
                &ToposQuery::create()
                    .genre("Pentateuch".into())
                    .inside("Gen 1".into())
            ),
            ["Genesis 1:1"]
        );
        assert_eq!(
            search(&nt.exclude_overlap("John 3".into())),
            ["Romans 8:28"]
        );

        let contradiction = ToposQuery::create()
            .old_testament()
            .genre("Pauline Epistles".into());
        assert!(topos.contradiction(&contradiction).unwrap().is_some());
        assert_eq!(topos.contradiction(&nt).unwrap(), None);

        let unknown = topos.search_with(
            text,
            OffsetUnit::Byte,
            &ToposQuery::create().book("Jhon".into()),
        );
        assert!(
            matches!(unknown, Err(ToposError::InvalidQuery { message }) if message.contains("Jhon"))
        );
    }

    #[test]
    fn formats_for_results_completions_and_passages() {
        let joined = ToposFormat::create()
            .book(BookStyle::Abbreviation)
            .join_adjacent(true)
            .range("\u{2013}".into());
        let topos = ToposOptions::create().format(&joined).build().unwrap();
        let found = topos.search("John 3:16, 17, 18 and Jude 1:5".into(), OffsetUnit::Byte);
        assert_eq!(references(found.clone()), ["Jn 3:16\u{2013}18", "Jude 1:5"]);

        // complete keeps its own book style; completeWith takes a whole format
        let text = String::from("jn 3:16-");
        let labels = |completions: Vec<Completion>| -> Vec<String> {
            completions.into_iter().map(|c| c.label).collect()
        };
        assert_eq!(
            labels(topos.complete(text.clone(), 8, OffsetUnit::Byte, BookStyle::Name, 1)),
            ["John 3:16\u{2013}17"]
        );
        let dots = ToposFormat::create().chapter_verse(".".into());
        assert_eq!(
            labels(Topos::new().complete_with(text, 8, OffsetUnit::Byte, &dots, 1)),
            ["John 3.16-17"]
        );

        // formatPassage, and the CLI's JSON (on top of the format it is called on)
        let jude = found[1].passage.clone();
        let short = ToposFormat::create()
            .with_json(r#"{"chapter_in_single_chapter_books": false}"#.into())
            .unwrap();
        assert_eq!(
            topos.format_passage(jude, &short).as_deref(),
            Some("Jude 5")
        );
        let both = joined
            .with_json(r#"{"chapter_verse": "."}"#.into())
            .unwrap();
        assert_eq!(
            topos
                .format_passage(found[0].passage.clone(), &both)
                .as_deref(),
            Some("Jn 3.16\u{2013}18")
        );
        let bad = ToposFormat::create().with_json(r#"{"joins": true}"#.into());
        assert!(
            matches!(bad, Err(ToposError::InvalidConfig { message }) if message.contains("joins"))
        );
    }

    #[test]
    fn options_merge_remove_and_set_context() {
        let topos = ToposOptions::create()
            .merge_data(r#"{ "books": [{ "book": "John", "abbreviations": ["jhn"] }] }"#.into())
            .remove_data(r#"{ "books": [{ "book": "Jude" }] }"#.into())
            .build()
            .unwrap();
        assert_eq!(
            references(topos.search("Jhn 3:16 and Jude 5".into(), OffsetUnit::Byte)),
            ["John 3:16"]
        );
        let conflict = ToposOptions::create()
            .merge_data(r#"{ "books": [{ "book": "John", "abbreviations": ["gen"] }] }"#.into())
            .build();
        assert!(
            matches!(conflict, Err(ToposError::InvalidConfig { message }) if message.contains("`gen`"))
        );

        let john = ToposOptions::create()
            .context_book("John".into())
            .build()
            .unwrap();
        assert_eq!(
            references(john.search("As 3:16 says".into(), OffsetUnit::Byte)),
            ["John 3:16"]
        );
        // The context applies to queries too
        let query = ToposQuery::create().explicit_overlap("John 3".into());
        assert_eq!(
            references(
                john.search_with("As 3:16 says".into(), OffsetUnit::Byte, &query)
                    .unwrap()
            ),
            ["John 3:16"]
        );
        let headings = ToposOptions::create()
            .context_heading("^# {book}$".into())
            .build()
            .unwrap();
        assert_eq!(
            references(headings.search("# Romans\nSee 8:28".into(), OffsetUnit::Byte)),
            ["Romans 8:28"]
        );
    }

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
    fn parses_osis_ranges() {
        let topos = Topos::new();
        let range = topos.parse("John.3.16-John.3.18 John.4".into(), BookStyle::Name);
        assert_eq!(range.unwrap().reference, "John 3:16-18; 4");
        let plain = topos.parse("Jn 3:16-18".into(), BookStyle::Name);
        assert_eq!(plain.unwrap().reference, "John 3:16-18");
    }

    #[test]
    fn custom_config() {
        let config = r#"{"books": [{"id": 1, "book": "Bereshit", "abbreviation": "Ber"}]}"#;
        let topos = Topos::with_config(config.into()).unwrap();
        assert_eq!(topos.search("Bereshit 1".into(), OffsetUnit::Byte).len(), 1);
        assert!(Topos::with_config("{".into()).is_err());
    }
}

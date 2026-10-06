use std::collections::{BTreeMap, BTreeSet};

use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{
    error::{ToposError, ToposResult},
    segments::{
        grammar::roman,
        passage::{Passage, Segments},
    },
};

/// This is not guaranteed to be a valid key, I just am using a unique type
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct BookId(pub u8);

// How do I want to do this so that I am not creating tons of strings? should I use Cow<String>s?
// pub struct BookInfo {
//     id: BookId,
//     name: String,
//     abbrev: String,
// }

/// eventually this will have a locale so i can group by languages
#[derive(Clone, Debug)]
pub struct Books {
    /// map of abbreviations and actual name (all lowercase) to book id (for searching)
    input_to_book_id: BTreeMap<String, BookId>,
    /// map of book id to book name (for display)
    book_id_to_name: BTreeMap<BookId, String>,
    /// map of book id to abbreviation (for display)
    book_id_to_abbreviation: BTreeMap<BookId, String>,
    /// map of book id to OSIS book id (`Gen`, `1Sam`)
    book_id_to_osis: BTreeMap<BookId, String>,
    /// map of OSIS book id to book id
    osis_to_book_id: BTreeMap<String, BookId>,
    /// normalized abbreviations that are also common words (`is`, `am`)
    ambiguous: BTreeSet<String>,
    regexes: BookRegexes,
}

/// Every regex alternates over all book names, escaped and longest first, so `1 John` wins over
/// `John` and `Song of Songs` over `Song`
#[derive(Clone, Debug)]
struct BookRegexes {
    /// The alternation of every book name (escaped, longest first), without flags or groups
    pattern: String,
    /// A book name for searching text (see [`Books::candidates`])
    candidate: Regex,
    /// A book on its own, for autocomplete
    book: Regex,
    /// A book and everything after it, for parsing a single passage
    passage: Regex,
}

impl BookRegexes {
    fn new<'a>(keys: impl Iterator<Item = &'a String>) -> Result<Self, regex::Error> {
        let mut keys: Vec<&String> = keys.collect();
        keys.sort_by_key(|k| std::cmp::Reverse(k.chars().count()));
        // Unicode word boundaries are ~100x slower, so only use them for non-ASCII book names
        let b = if keys.iter().all(|k| k.is_ascii()) {
            r"(?-u:\b)"
        } else {
            r"\b"
        };
        let books = keys.into_iter().map(|k| regex::escape(k)).join("|");
        Ok(Self {
            pattern: books.clone(),
            candidate: Regex::new(&format!(r"(?i){b}((?:{books})\.?)"))?,
            book: Regex::new(&format!(r"(?i){b}((?:{books}){b}\.?)"))?,
            passage: Regex::new(&format!(r"(?i){b}((?:{books})){b}\.?(.*)"))?,
        })
    }
}

impl Books {
    fn key_to_id(&self) -> &BTreeMap<String, BookId> {
        &self.input_to_book_id
    }
    fn id_to_name(&self) -> &BTreeMap<BookId, String> {
        &self.book_id_to_name
    }
    fn id_to_abbrev(&self) -> &BTreeMap<BookId, String> {
        &self.book_id_to_abbreviation
    }
}

impl Books {
    pub fn iter_keys_and_ids(&self) -> impl Iterator<Item = (&String, &BookId)> {
        self.key_to_id().iter()
    }
    /// Every book id, in order
    pub fn ids(&self) -> impl Iterator<Item = BookId> + '_ {
        self.id_to_name().keys().copied()
    }
    pub fn search(&self, name: &str) -> Option<BookId> {
        let name = Self::normalize_book_name(name);
        self.key_to_id().get(&name).cloned()
    }
    pub fn get_name(&self, id: BookId) -> Option<&String> {
        self.id_to_name().get(&id)
    }
    pub fn get_abbrev(&self, id: BookId) -> Option<&String> {
        self.id_to_abbrev().get(&id)
    }
    /// The OSIS book id, like `Gen` or `1Sam`
    pub fn get_osis(&self, id: BookId) -> Option<&String> {
        self.book_id_to_osis.get(&id)
    }
    /// The book for an OSIS book id (exact, case-sensitive)
    pub fn search_osis(&self, osis: &str) -> Option<BookId> {
        self.osis_to_book_id.get(osis).copied()
    }

    /**
    - Book names in `text` that are followed by something that could start a chapter (a digit or
      Roman numeral), including a trailing `.`
    - The chapter is not consumed, so in `AM 1 Samuel` the `1` can still start `1 Samuel`
    */
    pub fn candidates<'h>(&self, text: &'h str) -> impl Iterator<Item = regex::Match<'h>> {
        // The whole match is the book name, so `find_iter` avoids the slower capture engine
        self.regexes.candidate.find_iter(text).filter(move |m| {
            text[m.end()..]
                .trim_start()
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_digit() || roman::is_numeral(b))
        })
    }

    /// The alternation of every book name and abbreviation (escaped, longest first, lowercase)
    pub fn pattern(&self) -> &str {
        &self.regexes.pattern
    }

    /// Whether this name is an abbreviation that is also a common word (`is`, `am`)
    pub fn is_ambiguous(&self, name: &str) -> bool {
        self.ambiguous.contains(&Self::normalize_book_name(name))
    }

    /// Matches a book name on its own; group 1 is the book name, including a trailing `.`
    pub fn book_regex(&self) -> &Regex {
        &self.regexes.book
    }
}

impl Default for Books {
    fn default() -> Self {
        Self::base().clone()
    }
}

impl Books {
    /// - You only want to use this when you have custom data
    /// - If you would like English book names, please just use [`Default::default()`]
    pub fn new(data: BooksInput) -> ToposResult<Self> {
        let mut abbreviations_to_book_id = BTreeMap::new();
        let mut book_id_to_name = BTreeMap::new();
        let mut book_id_to_abbreviation = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();
        let mut book_id_to_osis = BTreeMap::new();
        let mut osis_to_book_id = BTreeMap::new();

        // Each name means one book, and each book has one id
        let mut owners: BTreeMap<String, (BookId, String)> = BTreeMap::new();
        for book in &data.0 {
            let names = std::iter::once(&book.book).chain(&book.abbreviations);
            for name in names {
                let key = Books::normalize_book_name(name);
                match owners.get(&key) {
                    Some((id, other)) if *id != book.id => {
                        return Err(ToposError::InvalidData(format!(
                            "`{name}` would mean both {other} and {}",
                            book.book
                        )));
                    }
                    _ => {
                        owners.insert(key, (book.id, book.book.clone()));
                    }
                }
            }
        }
        let mut ids = BTreeSet::new();
        if let Some(book) = data.0.iter().find(|book| !ids.insert(book.id)) {
            return Err(ToposError::InvalidData(format!(
                "book id {} is used more than once (by {})",
                book.id.0, book.book
            )));
        }

        for book in data.0 {
            if let Some(osis) = &book.osis {
                book_id_to_osis.insert(book.id, osis.clone());
                osis_to_book_id.insert(osis.clone(), book.id);
            }
            ambiguous.extend(book.ambiguous.iter().map(|a| Books::normalize_book_name(a)));
            abbreviations_to_book_id.insert(Books::normalize_book_name(&book.book), book.id);
            book_id_to_name.insert(book.id, book.book);
            book_id_to_abbreviation.insert(book.id, book.abbreviation);
            for abbreviation in book.abbreviations {
                abbreviations_to_book_id.insert(Books::normalize_book_name(&abbreviation), book.id);
            }
        }

        let regexes = BookRegexes::new(abbreviations_to_book_id.keys())
            .map_err(|e| ToposError::InvalidData(format!("book names do not form a regex: {e}")))?;

        Ok(Books {
            input_to_book_id: abbreviations_to_book_id,
            book_id_to_name,
            book_id_to_abbreviation,
            book_id_to_osis,
            osis_to_book_id,
            ambiguous,
            regexes,
        })
    }

    /// The book and the text of its chapters and verses in the first reference of `input`
    pub(crate) fn split_reference<'a>(&self, input: &'a str) -> Option<(BookId, &'a str)> {
        let m = self.regexes.passage.captures_iter(input).next()?;
        let book = self.search(m.get(1)?.as_str())?;
        Some((book, m.get(2)?.as_str()))
    }

    /**
    The first reference in `input`, without the book's chapter and verse counts, so a number in
    a single-chapter book is read as a chapter (`Jude 5` is chapter 5) and references that don't
    exist are accepted; prefer [`BibleData::parse`](crate::data::bible_data::BibleData::parse)
    */
    pub fn parse(&self, input: &str) -> Option<Passage> {
        let m = &self.regexes.passage.captures_iter(input).next()?;
        let book = m.get(1)?.as_str();
        let book = self.search(book)?;
        let segments = m.get(2)?.as_str();
        let segments = Segments::parse(segments)?;
        Some(segments.with_book(book))
    }

    pub fn normalize_book_name(name: &str) -> String {
        name.to_lowercase()
            .trim()
            .trim_end_matches(".")
            .trim()
            .to_string()
    }

    /// - This is a global reference to the default book data. If you want to clone it, just use
    ///   [`Default::default`]
    /// - This lets me reference it in other defaults without having to clone it
    pub fn base() -> &'static Self {
        &DEFAULT_BOOKS
    }
}

static DEFAULT_BOOKS: Lazy<Books> = Lazy::new(|| {
    let data = BooksInput::default();
    Books::new(data).expect("the default book data is valid")
});

/**
Example:
```jsonc
[
  {
    "id": 1,
    "book": "Genesis",
    "abbreviation": "Gn",
    "abbreviations": [
      "gen",
      "ge",
      "gn"
    ]
  },
  // ...
]
```
*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    /// - book id, starting at 1
    /// - Genesis = 1
    /// - Matthew = 40
    #[serde(alias = "num")]
    #[serde(alias = "number")]
    pub(crate) id: BookId,

    /// - the display name
    /// - case is kept
    /// - does not need to be repeated in abbreviations
    #[serde(alias = "name")]
    #[serde(alias = "book_name")]
    #[serde(alias = "display_name")]
    pub(crate) book: String,

    /// - the display abbreviation
    /// - case is kept
    /// - does not need to be repeated in abbreviations
    /// - TODO: if not provided, the first abbreviations as title case; do that by changing this to
    ///   a BookInput struct
    #[serde(alias = "abbr")]
    #[serde(alias = "abbrv")]
    #[serde(alias = "abbrev")]
    pub(crate) abbreviation: String,

    /// - the OSIS book id, like `Gen`, `1Sam`, or `John`
    /// - used to read and write OSIS references
    #[serde(default)]
    pub(crate) osis: Option<String>,

    /// - does not need to include book name or abbreviation
    /// - meant for matching/parsing references
    #[serde(alias = "abbrs")]
    #[serde(alias = "abbrvs")]
    #[serde(alias = "abbrevs")]
    #[serde(default)]
    pub(crate) abbreviations: Vec<String>,

    /// - abbreviations that are also common words, like `is` for Isaiah
    /// - in search, these only match with an explicit verse (`Is 1:1`, but not `is 1 of`)
    #[serde(default)]
    pub(crate) ambiguous: Vec<String>,
}

// static DEFAULT_BOOKS_JSON: &'static str = include_str!(concat!(
//     env!("CARGO_MANIFEST_DIR"),
//     "/src/data/default_books.json"
// ));

static DEFAULT_BOOKS_JSON: &str = include_str!("./default_books.json");

#[derive(Clone, Debug, Serialize, Deserialize)]
// #[derive(Deref, DerefMut, IntoIterator)]
pub struct BooksInput(pub(crate) Vec<Book>);

impl Default for BooksInput {
    fn default() -> Self {
        serde_json::from_str(DEFAULT_BOOKS_JSON).expect("the default book data is valid JSON")
    }
}

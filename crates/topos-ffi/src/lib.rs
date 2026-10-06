//! The API that every language binding sees: owned data, one object, and no lifetimes.
//!
//! Offsets are reported in the unit the caller asks for, since languages index strings
//! differently: bytes (Rust, C), Unicode scalars (Python), or UTF-16 code units (JavaScript,
//! Kotlin, Swift's `utf16` view).

use boltffi::*;
use topos_lib::{
    data::bible_data::{BibleData, BibleDataInput},
    filter::bible_filter::BibleFilter,
    matcher::BibleMatcher,
    segments::{
        Passage as CorePassage,
        autocomplete::{CompleteOptions, CompletionKind as CoreKind},
        formatter::{BookStyle as CoreBookStyle, FormatOptions},
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

#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passage {
    /// 1 for Genesis through 66 for Revelation (with the default data)
    pub book_id: u8,
    pub book: String,
    /// `John 3:16-18`
    pub reference: String,
    /// `John.3.16-John.3.18`
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
    fn passage(&self, passage: &CorePassage, style: CoreBookStyle) -> Option<Passage> {
        let data = self.matcher.data();
        let options = FormatOptions {
            book: style,
            ..FormatOptions::default()
        };
        Some(Passage {
            book_id: passage.book.0,
            book: data.books().get_name(passage.book)?.clone(),
            reference: options.passage(passage, data)?,
            osis: passage.to_osis(data.books()).unwrap_or_default(),
        })
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

    #[test]
    fn custom_config() {
        let config = r#"{"books": [{"id": 1, "book": "Bereshit", "abbreviation": "Ber"}]}"#;
        let topos = Topos::with_config(config.into()).unwrap();
        assert_eq!(topos.search("Bereshit 1".into(), OffsetUnit::Byte).len(), 1);
        assert!(Topos::with_config("{".into()).is_err());
    }
}

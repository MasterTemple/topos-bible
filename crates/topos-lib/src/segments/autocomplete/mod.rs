//! Completing a reference while it is typed, as edits that a text box or an LSP server applies.

mod incomplete;
mod segments;

use std::ops::Range;

use itertools::Itertools;

use crate::{
    data::{bible_data::BibleData, books::BookId},
    matcher::BibleMatcher,
    segments::{
        formatter::{BookStyle, FormatOptions},
        passage::Passage,
        segment::Segment,
    },
};

/// How completions are written
#[derive(Clone, Debug, Default)]
pub struct CompleteOptions {
    /// Book style and separators of the completed text, so `gen 1:1,2` can become
    /// `Genesis 1:1, 2` by setting `book` and `verse_separator`
    pub format: FormatOptions,
    /// At most this many completions
    pub limit: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Book,
    Chapter,
    Verse,
}

/// Replace the text at `range` (byte offsets into the input) with `text`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range<usize>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// What a menu shows (`Genesis 1:3`)
    pub label: String,
    pub kind: CompletionKind,
    /// Apply this to the input to accept the completion
    pub edit: TextEdit,
    /// The passage the input becomes, for chapter and verse completions
    pub passage: Option<Passage>,
}

impl BibleMatcher {
    /**
    Completions for the reference that ends at `cursor` (a byte offset into `text`)
    - Book names complete from a prefix of any name or abbreviation (`1 Co` → `1 Corinthians`)
    - Chapters and verses complete from the book's versification
    - Each edit rewrites the whole reference in the style of [`CompleteOptions::format`]
    - For LSP, convert edit ranges with [`LineIndex`](crate::matcher::LineIndex), whose positions
      have UTF-16 columns
    */
    pub fn complete(
        &self,
        text: &str,
        cursor: usize,
        options: &CompleteOptions,
    ) -> Vec<Completion> {
        if !text.is_char_boundary(cursor) {
            return vec![];
        }
        let before = &text[..cursor];
        let mut completions = book_completions(self.data(), before, &options.format);
        completions.extend(segment_completions(self.data(), before, &options.format));
        if let Some(limit) = options.limit {
            completions.truncate(limit);
        }
        completions
    }
}

fn segment_completions(data: &BibleData, before: &str, format: &FormatOptions) -> Vec<Completion> {
    let Some(found) = segments::suggest_segments(data, before) else {
        return vec![];
    };
    // The number being typed narrows the suggestions: `John 2` offers 2, 20, and 21
    let typed = &before[before.trim_end_matches(|c: char| c.is_ascii_digit()).len()..];
    found
        .suggestions
        .into_iter()
        .filter_map(|suggestion| {
            let passage = found
                .segments
                .with_suggestion(suggestion)
                .with_book(found.book);
            let text = format.passage(&passage, data)?;
            if !last_number(&text).starts_with(typed) {
                return None;
            }
            let kind = match suggestion {
                Segment::FullChapter(_) | Segment::FullChapterRange(_) => CompletionKind::Chapter,
                _ => CompletionKind::Verse,
            };
            Some(Completion {
                label: text.clone(),
                kind,
                edit: TextEdit {
                    range: found.start..before.len(),
                    text,
                },
                passage: Some(passage),
            })
        })
        .collect()
}

/// The last run of digits in `text` (`16` in `John 3:16`)
fn last_number(text: &str) -> &str {
    let end = text
        .rfind(|c: char| c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    let start = text[..end]
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    &text[start..end]
}

/// The longest word suffix of `before` (up to 3 words) that starts some book name, and the books
fn typed_book_prefix(data: &BibleData, before: &str) -> Option<(usize, Vec<BookId>)> {
    // Only complete a book while a word with a letter is being typed (not `Genesis 1`)
    let last_word = before.rsplit(char::is_whitespace).next()?;
    if !last_word.chars().any(char::is_alphabetic) {
        return None;
    }
    let word_starts: Vec<usize> = before
        .char_indices()
        .filter(|&(idx, c)| {
            c.is_alphanumeric()
                && before[..idx]
                    .chars()
                    .next_back()
                    .is_none_or(|prev| !prev.is_alphanumeric() && prev != '.')
        })
        .map(|(idx, _)| idx)
        .collect();
    word_starts.iter().rev().take(3).rev().find_map(|&start| {
        let typed = before[start..].to_lowercase();
        let books: Vec<BookId> = data
            .books()
            .iter_keys_and_ids()
            .filter(|(key, _)| key.starts_with(&typed))
            .sorted_by_key(|(key, _)| key.len())
            .map(|(_, id)| *id)
            .unique()
            .collect();
        (!books.is_empty()).then_some((start, books))
    })
}

fn book_completions(data: &BibleData, before: &str, format: &FormatOptions) -> Vec<Completion> {
    let Some((start, books)) = typed_book_prefix(data, before) else {
        return vec![];
    };
    let typed = &before[start..];
    books
        .into_iter()
        .filter_map(|book| {
            let books = data.books();
            let name = match format.book {
                BookStyle::Name => books.get_name(book),
                BookStyle::Abbreviation => books.get_abbrev(book),
                BookStyle::Osis => books.get_osis(book),
            }?;
            // Typing the whole name already offers its chapters
            if name.eq_ignore_ascii_case(typed) {
                return None;
            }
            Some(Completion {
                label: name.clone(),
                kind: CompletionKind::Book,
                edit: TextEdit {
                    range: start..before.len(),
                    text: format!("{name}{}", format.book_separator),
                },
                passage: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(input: &str) -> Vec<String> {
        let matcher = BibleMatcher::default();
        matcher
            .complete(input, input.len(), &CompleteOptions::default())
            .into_iter()
            .map(|c| c.label)
            .collect()
    }

    #[test]
    fn completes_chapters_and_verses() {
        // Nothing typed yet: every chapter of Genesis
        assert_eq!(labels("Genesis ").len(), 50);
        // `1:` suggests the 31 verses of chapter 1
        let verses = labels("Gen. 1:");
        assert_eq!(verses.len(), 31);
        assert_eq!(verses[0], "Genesis 1:1");
        // `1:1-` suggests verse ends, then chapter ends
        let ranges = labels("Genesis 1:1-");
        assert_eq!(ranges[0], "Genesis 1:1-2");
        assert!(ranges.contains(&String::from("Genesis 1:1-2:1")));
        // Complete segments before the cursor are kept
        assert_eq!(labels("Genesis 1:1, ")[0], "Genesis 1:1,2");
        // Same lexer as search, so Roman numerals and other dashes work here too
        assert_eq!(labels("Genesis i:").len(), 31);
        assert_eq!(labels("Genesis 1:1–")[0], "Genesis 1:1-2");
    }

    #[test]
    fn narrows_by_the_number_being_typed() {
        // John has 21 chapters, and John 3 has 36 verses
        assert_eq!(labels("John 2"), ["John 2", "John 20", "John 21"]);
        assert_eq!(labels("John 3"), ["John 3"]);
        assert_eq!(
            labels("John 3:3"),
            [
                "John 3:3",
                "John 3:30",
                "John 3:31",
                "John 3:32",
                "John 3:33",
                "John 3:34",
                "John 3:35",
                "John 3:36"
            ]
        );
        assert_eq!(labels("John 3:1-2")[0], "John 3:1-2");
        assert_eq!(labels("John 3:").len(), 36);
    }

    #[test]
    fn completes_books() {
        assert_eq!(&labels("see 1 C")[..2], ["1 Chronicles", "1 Corinthians"]);
        // `1 Co` is an abbreviation itself, so its chapters follow
        let one_co = labels("see 1 Co");
        assert_eq!(&one_co[..2], ["1 Corinthians", "1 Corinthians 1"]);
        assert_eq!(&labels("Phil")[..2], ["Philippians", "Philemon"]);
        // An abbreviation offers the full name, then its chapters
        let genesis = labels("Gen");
        assert_eq!(genesis[0], "Genesis");
        assert_eq!(genesis[1], "Genesis 1");
        // A number is a chapter, not the start of `1 Samuel`
        assert!(labels("Genesis 1").iter().all(|l| l.starts_with("Genesis")));
    }

    #[test]
    fn edits_replace_the_reference() {
        let matcher = BibleMatcher::default();
        let options = CompleteOptions {
            format: FormatOptions {
                verse_separator: String::from(", "),
                ..FormatOptions::default()
            },
            limit: Some(1),
        };
        let input = "Read gen 1:1,";
        let completions = matcher.complete(input, input.len(), &options);
        let edit = &completions[0].edit;
        assert_eq!(edit.range, 5..input.len());
        assert_eq!(edit.text, "Genesis 1:1, 2");

        let book = &matcher.complete("Read 1 Co", 9, &CompleteOptions::default())[0];
        assert_eq!(
            (book.edit.range.clone(), book.edit.text.as_str()),
            (5..9, "1 Corinthians ")
        );
    }

    #[test]
    fn nothing_outside_a_reference() {
        assert!(labels("Genesis 1:1 and then").is_empty());
        assert!(labels("no book here 5").is_empty());
        assert!(
            BibleMatcher::default()
                .complete("Jé", 2, &CompleteOptions::default())
                .is_empty()
        );
    }
}

use std::{borrow::Cow, ops::Range, sync::LazyLock};

use regex::Regex;

/// A hyphen (or soft hyphen) that splits a word across lines, as in justified text
static HYPHENATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[-\u{2010}\u{AD}][ \t]*\r?\n[ \t]*|\u{AD}").unwrap());

/**
The text that is actually searched, with a map back to byte offsets in the original
- Words split across lines (`Ephe-\nsians`) are joined, so book names still match
- Only hyphens between two letters are removed, so ranges like `3:16-\n18` are untouched
- When nothing is removed, the original text is borrowed as is
*/
#[derive(Clone, Debug)]
pub struct SearchText<'a> {
    text: Cow<'a, str>,
    /// `(offset in text, bytes removed before it)`, sorted by offset
    shifts: Vec<(usize, usize)>,
}

impl<'a> SearchText<'a> {
    pub fn new(original: &'a str) -> Self {
        let mut text = String::new();
        let mut shifts = vec![];
        let mut copied = 0;
        let mut removed = 0;
        for m in HYPHENATION.find_iter(original) {
            let joins_letters = original[..m.start()]
                .chars()
                .next_back()
                .is_some_and(char::is_alphabetic)
                && original[m.end()..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphabetic);
            if !joins_letters {
                continue;
            }
            text.push_str(&original[copied..m.start()]);
            copied = m.end();
            removed += m.len();
            shifts.push((text.len(), removed));
        }

        if shifts.is_empty() {
            return Self {
                text: Cow::Borrowed(original),
                shifts,
            };
        }
        text.push_str(&original[copied..]);
        Self {
            text: Cow::Owned(text),
            shifts,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The byte offset in the original text for an offset in [`SearchText::as_str`]
    pub fn original(&self, offset: usize) -> usize {
        let idx = self.shifts.partition_point(|&(at, _)| at <= offset);
        match idx.checked_sub(1) {
            Some(idx) => offset + self.shifts[idx].1,
            None => offset,
        }
    }

    pub fn original_range(&self, range: Range<usize>) -> Range<usize> {
        self.original(range.start)..self.original(range.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_hyphenated_words() {
        let original = "See Ephe-\n  sians 2:8 and Deu\u{AD}teronomy 6:4";
        let text = SearchText::new(original);
        assert_eq!(text.as_str(), "See Ephesians 2:8 and Deuteronomy 6:4");

        let at = text.as_str().find("2:8").unwrap();
        assert_eq!(&original[text.original_range(at..at + 3)], "2:8");
        let at = text.as_str().find("6:4").unwrap();
        assert_eq!(&original[text.original_range(at..at + 3)], "6:4");
        // A range that spans the joined word maps to the original span
        let book = text.as_str().find("Ephesians").unwrap();
        assert_eq!(
            &original[text.original_range(book..book + "Ephesians".len())],
            "Ephe-\n  sians"
        );
    }

    #[test]
    fn leaves_other_hyphens() {
        for original in ["John 3:16-\n18", "well-known", "a -\n b", "no hyphens"] {
            let text = SearchText::new(original);
            assert_eq!(text.as_str(), original);
            assert!(matches!(text.text, Cow::Borrowed(_)));
        }
    }
}

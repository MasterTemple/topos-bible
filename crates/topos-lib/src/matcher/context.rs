use std::{ops::Range, sync::LazyLock};

use regex::Regex;

use crate::{
    data::{
        bible_data::BibleData,
        books::{BookId, Books},
    },
    matcher::instance::FoundPassage,
};

/// The start of a bare reference: at least `chapter:verse`
static BARE_REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u:\b)\d{1,3}[ \t]*:[ \t]*\d").expect("valid regex"));

/**
Which book a reference without a book name (`1:1-5`) belongs to

Bare references are only matched with an explicit `chapter:verse`, so lone numbers are never
taken as references.
*/
#[derive(Clone, Debug)]
pub enum BookContext {
    /// The whole document is about this book (like a commentary on John)
    Book(BookId),
    /// Lines matching this pattern name the book for everything after them, until the next one
    Headings(Regex),
}

impl BookContext {
    /**
    - `pattern` is a regex with `{book}` where the book name goes, like `^#+ {book}$`
    - It is matched per line (`^` and `$` are line boundaries) and book names ignore case
    */
    pub fn headings(books: &Books, pattern: &str) -> Result<Self, regex::Error> {
        let book = format!("(?<book>(?i:{}))", books.pattern());
        let pattern = format!("(?m){}", pattern.replace("{book}", &book));
        Ok(Self::Headings(Regex::new(&pattern)?))
    }

    /// `(offset, book)` pairs: the book in context from each offset on, sorted by offset
    fn regions(&self, books: &Books, text: &str) -> Vec<(usize, BookId)> {
        match self {
            BookContext::Book(book) => vec![(0, *book)],
            BookContext::Headings(regex) => regex
                .captures_iter(text)
                .filter_map(|cap| {
                    let book = books.search(cap.name("book")?.as_str())?;
                    Some((cap.get(0)?.end(), book))
                })
                .collect(),
        }
    }

    /**
    Bare references in `text` that are not inside `taken` (references that name their book)
    - `book_starts` are the sorted starts of book names, which end a bare reference's segments
    */
    pub(crate) fn find_bare(
        &self,
        data: &BibleData,
        text: &str,
        taken: &[Range<usize>],
        book_starts: &[usize],
    ) -> Vec<FoundPassage> {
        let regions = self.regions(data.books(), text);
        let mut found: Vec<FoundPassage> = vec![];
        for m in BARE_REFERENCE.find_iter(text) {
            let start = m.start();
            let inside = |r: &Range<usize>| r.contains(&start);
            if taken.iter().any(inside) || found.iter().any(|f| inside(&f.bytes)) {
                continue;
            }
            let Some(&(_, book)) = regions.iter().rev().find(|(at, _)| *at <= start) else {
                continue;
            };
            let end = book_starts
                .get(book_starts.partition_point(|&s| s <= start))
                .copied()
                .unwrap_or(text.len());
            if let Some(passage) = FoundPassage::find_bare(data, book, text, start..end) {
                found.push(passage);
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::BibleMatcher;

    fn search(context: impl FnOnce(&Books) -> BookContext, input: &str) -> Vec<String> {
        let matcher = BibleMatcher::default();
        let context = context(matcher.data().books());
        let matcher = matcher.with_context(context);
        matcher
            .search(input)
            .into_iter()
            .map(|m| {
                let book = matcher.data().books().get_name(m.psg.book).unwrap();
                format!("{book} {}", m.psg.segments)
            })
            .collect()
    }

    #[test]
    fn whole_document_about_a_book() {
        let john = |_: &Books| BookContext::Book(BookId(43));
        let doc = "## 1:1-5\nIn the beginning (see Gen 1:1)...\n## 1:6-8, 10\nOn day 3 at 10:30";
        assert_eq!(
            search(john, doc),
            ["John 1:1-5", "Genesis 1:1", "John 1:6-8,10", "John 10:30"]
        );
        // A lone number is never a reference
        assert!(search(john, "chapter 3 has 36 verses").is_empty());
    }

    #[test]
    fn headings_name_the_book() {
        let headings = |books: &Books| BookContext::headings(books, "^# {book}$").unwrap();
        let doc = "1:1 before any heading\n# John\n## 3:16\n# Romans\n## 8:28\ntext 5:1";
        assert_eq!(
            search(headings, doc),
            ["John 3:16", "Romans 8:28", "Romans 5:1"]
        );
    }

    #[test]
    fn named_references_are_not_matched_twice() {
        let john = |_: &Books| BookContext::Book(BookId(43));
        assert_eq!(search(john, "Romans 8:28-9:1"), ["Romans 8:28-9:1"]);
    }
}

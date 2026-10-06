use itertools::Itertools;
use regex::Regex;

use crate::data::chapter_verses::BookChapterVerses;

use super::{books::Books, genres::Genres};

#[derive(Clone, Debug, Default)]
pub struct BibleData {
    books: Books,
    genres: Genres,
    chapter_verses: BookChapterVerses, // testaments: Test
}

impl BibleData {
    pub fn books(&self) -> &Books {
        &self.books
    }

    pub fn genres(&self) -> &Genres {
        &self.genres
    }

    pub fn chapter_verses(&self) -> &BookChapterVerses {
        &self.chapter_verses
    }

    pub fn create_book_regex(&self) -> Result<Regex, String> {
        let books_pattern: String = self
            .books()
            .iter_keys_and_ids()
            .map(|(key, _id)| key)
            .join("|");

        let book_regex = Regex::new(format!(r"\b(((?:)(?i){books_pattern})\b\.?)").as_str())
            .map_err(|e| format!("Failed to compile book_regex because of bad user input.\n{e}"))?;

        Ok(book_regex)
    }

    // pub fn parse(&self, input: &str) -> Option<BookSegments> {
    //
    // }
}

// impl BibleData {
//     pub fn base() -> &'static Self {
//         &DEFAULT_DATA
//     }
// }
//
// static DEFAULT_DATA: Lazy<BibleData> = Lazy::new(|| BibleData {
//     books: Books::base().clone(),
//     genres: Genres::default(),
// });

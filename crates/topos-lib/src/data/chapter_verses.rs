use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::data::books::{BookId, Books};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChapterVerses(Vec<u8>);

impl ChapterVerses {
    pub fn get_last_verse(&self, chapter: u8) -> Option<u8> {
        let idx = chapter.checked_sub(1)? as usize;
        self.0.get(idx).cloned()
    }

    pub fn has_one_chapter(&self) -> bool {
        self.0.len() == 1
    }

    pub fn get_chapter_count(&self) -> u8 {
        self.0.len() as u8
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookChapterVerses(BTreeMap<BookId, ChapterVerses>);

impl BookChapterVerses {
    pub fn create(books: &Books, input: BookChapterVersesInput) -> Self {
        Self(
            input
                .0
                .into_iter()
                .filter_map(|(name, v)| books.search(&name).map(|book| (book, v)))
                .collect(),
        )
    }

    pub fn get_chapter_verses(&self, book: &BookId) -> Option<&ChapterVerses> {
        self.0.get(book)
    }

    // pub fn get_last_verse(&self, book: &BookId, chapter: u8) -> Option<u8> {
    //     self.0.get(book)?.get_last_verse(chapter)
    // }
}

impl Default for BookChapterVerses {
    fn default() -> Self {
        Self::create(Books::base(), BookChapterVersesInput::default())
    }
}

/**
Example:
```jsonc
{
  "Genesis": [31, 25, 24, 26, 32, 22, 24, 22, 29, 32, 32, 20, 18, 24, 21, 16, 27, 33, 38, 18, 34, 24, 20, 67, 34, 35, 46, 22, 35, 43, 55, 32, 20, 31, 29, 43, 36, 30, 23, 23, 57, 38, 34, 34, 28, 34, 31, 22, 33, 26],
  // ...
}
```
*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BookChapterVersesInput(BTreeMap<String, ChapterVerses>);

static DEFAULT_CHAPTER_VERSES_JSON: &str = include_str!("./default_chapter_verses.json");

impl Default for BookChapterVersesInput {
    fn default() -> Self {
        serde_json::from_str(DEFAULT_CHAPTER_VERSES_JSON)
            .map_err(|_| "Could not parse default chapter verses file".to_string())
            .unwrap()
    }
}

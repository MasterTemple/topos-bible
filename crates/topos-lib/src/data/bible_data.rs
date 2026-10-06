use serde::{Deserialize, Serialize};

use crate::{
    data::{
        books::{Books, BooksInput},
        chapter_verses::{BookChapterVerses, BookChapterVersesInput},
        genres::{Genres, GenresInput},
    },
    error::ToposResult,
};

#[derive(Clone, Debug, Default)]
pub struct BibleData {
    books: Books,
    genres: Genres,
    chapter_verses: BookChapterVerses,
}

/**
Custom data, as read from a JSON config; anything left out uses the defaults

```jsonc
{
  "books": [{ "id": 1, "book": "Genesis", "abbreviation": "Gn", "abbreviations": ["gen"] }],
  "genres": [{ "title": "Torah", "abbreviations": ["law"], "books": ["Genesis"] }],
  "chapter_verses": { "Genesis": [31, 25] }
}
```
*/
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BibleDataInput {
    #[serde(default)]
    pub books: Option<BooksInput>,
    #[serde(default)]
    pub genres: Option<GenresInput>,
    #[serde(default)]
    pub chapter_verses: Option<BookChapterVersesInput>,
}

impl BibleData {
    /// Genres and chapter counts refer to books by name, so they are read against these books
    pub fn new(input: BibleDataInput) -> ToposResult<Self> {
        let books = match input.books {
            Some(books) => Books::new(books)?,
            None => Books::default(),
        };
        let genres = Genres::create(&books, input.genres.unwrap_or_default());
        let chapter_verses =
            BookChapterVerses::create(&books, input.chapter_verses.unwrap_or_default());
        Ok(Self {
            books,
            genres,
            chapter_verses,
        })
    }

    pub fn books(&self) -> &Books {
        &self.books
    }

    pub fn genres(&self) -> &Genres {
        &self.genres
    }

    pub fn chapter_verses(&self) -> &BookChapterVerses {
        &self.chapter_verses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_data_from_json() {
        let input: BibleDataInput = serde_json::from_str(
            r#"{
                "books": [{ "id": 1, "book": "Genesis", "abbreviation": "Gn", "abbreviations": ["bereshit"] }],
                "chapter_verses": { "Genesis": [31, 25] }
            }"#,
        )
        .unwrap();
        let data = BibleData::new(input).unwrap();
        let genesis = data.books().search("Bereshit").unwrap();
        assert_eq!(data.books().search("Exodus"), None);
        let versification = data.chapter_verses().get_chapter_verses(&genesis).unwrap();
        assert_eq!(versification.get_chapter_count(), 2);
    }
}

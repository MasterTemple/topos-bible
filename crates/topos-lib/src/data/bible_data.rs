use serde::{Deserialize, Serialize};

use crate::{
    data::{
        books::{Books, BooksInput},
        chapter_verses::{BookChapterVerses, BookChapterVersesInput},
        genres::{Genres, GenresInput},
    },
    error::ToposResult,
    segments::{Passage, grammar::SegmentList, resolve::Resolver},
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

    /**
    One reference, like `jn 3:16-18` or `Jude 5`, resolved with the book's chapters and verses
    the way search does: `Jude 5` is verse 5 (Jude has one chapter), and a reference that doesn't
    exist (`John 3:99`) is [`None`]; parts after one that doesn't exist are ignored, like search
    */
    pub fn parse(&self, input: &str) -> Option<Passage> {
        let (book, segments) = self.books.split_reference(input)?;
        let list = SegmentList::parse(segments);
        let versification = self.chapter_verses.get_chapter_verses(&book);
        let resolved = Resolver::for_book(versification).resolve(&list.nodes);
        (!resolved.segments.is_empty()).then(|| resolved.segments.with_book(book))
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
    #[test]
    fn parses_like_search() {
        let data = BibleData::default();
        let parse = |input: &str| {
            let passage = data.parse(input)?;
            Some(format!(
                "{} {}",
                data.books().get_name(passage.book)?,
                passage.segments
            ))
        };
        // A single-chapter book's number is a verse
        assert_eq!(parse("Jude 5").as_deref(), Some("Jude 1:5"));
        assert_eq!(parse("jn 3:16-18").as_deref(), Some("John 3:16-18"));
        assert_eq!(parse("John 3").as_deref(), Some("John 3"));
        // Like search, references that don't exist aren't references
        assert_eq!(parse("John 3:99"), None);
        assert_eq!(parse("John 3:16, 4:99").as_deref(), Some("John 3:16"));
        assert_eq!(parse("not a reference"), None);
    }

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

//! Changes to Bible data: merge more names into it, or remove some (the CLI's `--merge-data` and
//! `--remove-data`). A patch has the same shape as custom data, but every field is optional.
//!
//! ```jsonc
//! // merge: John also matches `jhn`; a new book; a genre gains a book
//! {
//!   "books": [
//!     { "book": "John", "abbreviations": ["jhn"] },
//!     { "id": 67, "book": "Tobit", "abbreviation": "Tob", "abbreviations": ["tb"] }
//!   ],
//!   "genres": [{ "title": "Gospels", "books": ["Acts"] }],
//!   "chapter_verses": { "Tobit": [22, 14, 17, 21, 22, 18, 16, 21, 6, 13, 18, 22, 18, 15] }
//! }
//! // remove: Song of Solomon no longer matches `song`; drop a genre entirely
//! {
//!   "books": [{ "book": "Song of Solomon", "abbreviations": ["song"] }],
//!   "genres": [{ "title": "Wisdom And Poetry" }]
//! }
//! ```

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    data::{
        bible_data::BibleDataInput,
        books::{Book, BookId, Books, BooksInput},
        chapter_verses::{BookChapterVersesInput, ChapterVerses},
        genres::{GenreInput, GenresInput},
    },
    error::{ToposError, ToposResult},
};

/// Custom data where every field is optional (see the module docs)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DataPatch {
    #[serde(default)]
    pub books: Vec<BookPatch>,
    #[serde(default)]
    pub genres: Vec<GenrePatch>,
    #[serde(default)]
    pub chapter_verses: BTreeMap<String, ChapterVerses>,
}

/// A book in a patch: found by `id`, else by any of its names
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BookPatch {
    #[serde(default, alias = "num", alias = "number")]
    pub id: Option<BookId>,
    #[serde(default, alias = "name", alias = "book_name", alias = "display_name")]
    pub book: Option<String>,
    #[serde(default, alias = "abbr", alias = "abbrv", alias = "abbrev")]
    pub abbreviation: Option<String>,
    #[serde(default)]
    pub osis: Option<String>,
    #[serde(default, alias = "abbrs", alias = "abbrvs", alias = "abbrevs")]
    pub abbreviations: Vec<String>,
    #[serde(default)]
    pub ambiguous: Vec<String>,
}

/// A genre in a patch: found by its title or an abbreviation
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GenrePatch {
    pub title: String,
    #[serde(default)]
    pub abbreviations: Vec<String>,
    #[serde(default)]
    pub books: Vec<String>,
    #[serde(default)]
    pub subcategories: Vec<String>,
}

fn key(name: &str) -> String {
    Books::normalize_book_name(name)
}

/// Adds the values not already there (compared like names are matched)
fn add_unique(list: &mut Vec<String>, values: &[String], except: &[&str]) {
    for value in values {
        let k = key(value);
        let taken = list.iter().any(|v| key(v) == k) || except.iter().any(|e| key(e) == k);
        if !taken {
            list.push(value.clone());
        }
    }
}

fn remove_values(list: &mut Vec<String>, values: &[String]) {
    let keys: BTreeSet<String> = values.iter().map(|v| key(v)).collect();
    list.retain(|v| !keys.contains(&key(v)));
}

impl BibleDataInput {
    /// The built-in data, with every part filled in (a base to merge into or remove from)
    pub fn defaults() -> Self {
        Self {
            books: Some(BooksInput::default()),
            genres: Some(GenresInput::default()),
            chapter_verses: Some(BookChapterVersesInput::default()),
        }
    }

    /**
    Adds a patch's names and values, keeping what is already there
    - A book found by id or name gains its abbreviations and ambiguous marks (each once); a name,
      display abbreviation, or OSIS id replaces the old one, and a renamed book keeps its old name
      as an abbreviation, so genres and chapter counts that use it still find it
    - A book that isn't found is added, and needs an `id`
    - Genres gain abbreviations, books, and subcategories; new genres are added
    - Chapter counts replace the book's
    - Every name must still mean one book (see [`Books::new`])
    */
    pub fn merge(&mut self, patch: DataPatch) -> ToposResult<()> {
        let books = &mut self.books.get_or_insert_with(BooksInput::default).0;
        for p in patch.books {
            match find_book(books, &p) {
                Some(idx) => {
                    let book = &mut books[idx];
                    if let Some(name) = p.book.filter(|name| key(name) != key(&book.book)) {
                        let old = std::mem::replace(&mut book.book, name);
                        add_unique(&mut book.abbreviations, &[old], &[&book.book]);
                    }
                    if let Some(abbreviation) = p.abbreviation {
                        book.abbreviation = abbreviation;
                    }
                    if p.osis.is_some() {
                        book.osis = p.osis;
                    }
                    let name = book.book.clone();
                    add_unique(&mut book.abbreviations, &p.abbreviations, &[&name]);
                    add_unique(&mut book.ambiguous, &p.ambiguous, &[]);
                }
                None => {
                    let name = p
                        .book
                        .clone()
                        .or(p.abbreviation.clone())
                        .unwrap_or_default();
                    let (Some(id), Some(book)) = (p.id, p.book) else {
                        return Err(ToposError::InvalidData(format!(
                            "book `{name}` isn't in the data: give it an `id` and a `book` name to add it"
                        )));
                    };
                    let mut abbreviations = vec![];
                    add_unique(&mut abbreviations, &p.abbreviations, &[&book]);
                    let mut ambiguous = vec![];
                    add_unique(&mut ambiguous, &p.ambiguous, &[]);
                    books.push(Book {
                        id,
                        abbreviation: p.abbreviation.unwrap_or_else(|| book.clone()),
                        book,
                        osis: p.osis,
                        abbreviations,
                        ambiguous,
                    });
                }
            }
        }
        let resolver = Books::new(self.books.clone().unwrap_or_default())?;

        let genres = &mut self.genres.get_or_insert_with(GenresInput::default).0;
        for p in patch.genres {
            match find_genre(genres, &p.title) {
                Some(idx) => {
                    let genre = &mut genres[idx];
                    let title = genre.title.clone();
                    add_unique(&mut genre.abbreviations, &p.abbreviations, &[&title]);
                    let books = genre.books.get_or_insert_default();
                    for book in p.books {
                        let id = resolver.search(&book);
                        if !books
                            .iter()
                            .any(|b| resolver.search(b) == id && id.is_some())
                        {
                            books.push(book);
                        }
                    }
                    add_unique(
                        genre.subcategories.get_or_insert_default(),
                        &p.subcategories,
                        &[],
                    );
                }
                None => genres.push(GenreInput {
                    title: p.title,
                    abbreviations: p.abbreviations,
                    books: Some(p.books),
                    subcategories: Some(p.subcategories),
                }),
            }
        }

        let counts = &mut self
            .chapter_verses
            .get_or_insert_with(BookChapterVersesInput::default)
            .0;
        for (name, verses) in patch.chapter_verses {
            // Replaces the book's counts, whatever name they were listed under
            let id = resolver.search(&name);
            counts.retain(|other, _| id.is_none() || resolver.search(other) != id);
            counts.insert(name, verses);
        }
        // Checks that every name still means one book
        Books::new(self.books.clone().unwrap_or_default())?;
        Ok(())
    }

    /**
    Removes what a patch lists
    - A book or genre listed with nothing else is removed entirely
    - Otherwise just the values listed are removed: a book's abbreviations and ambiguous marks; a
      genre's abbreviations, books, and subcategories
    - A book in `chapter_verses` loses its chapter and verse counts
    - Naming a book or genre that isn't in the data is an error, so typos don't go unnoticed
    */
    pub fn remove(&mut self, patch: DataPatch) -> ToposResult<()> {
        let resolver = Books::new(self.books.clone().unwrap_or_default())?;
        let books = &mut self.books.get_or_insert_with(BooksInput::default).0;
        for p in patch.books {
            let name = p
                .book
                .clone()
                .or(p.abbreviation.clone())
                .unwrap_or_default();
            let Some(idx) = find_book(books, &p) else {
                return Err(ToposError::InvalidData(format!(
                    "can't remove book `{name}`: it isn't in the data"
                )));
            };
            if p.abbreviations.is_empty() && p.ambiguous.is_empty() {
                books.remove(idx);
            } else {
                remove_values(&mut books[idx].abbreviations, &p.abbreviations);
                remove_values(&mut books[idx].ambiguous, &p.ambiguous);
            }
        }

        let genres = &mut self.genres.get_or_insert_with(GenresInput::default).0;
        for p in patch.genres {
            let Some(idx) = find_genre(genres, &p.title) else {
                return Err(ToposError::InvalidData(format!(
                    "can't remove genre `{}`: it isn't in the data",
                    p.title
                )));
            };
            if p.abbreviations.is_empty() && p.books.is_empty() && p.subcategories.is_empty() {
                genres.remove(idx);
                continue;
            }
            let genre = &mut genres[idx];
            remove_values(&mut genre.abbreviations, &p.abbreviations);
            if let Some(books) = &mut genre.books {
                let ids: BTreeSet<_> = p.books.iter().filter_map(|b| resolver.search(b)).collect();
                books.retain(|b| resolver.search(b).is_none_or(|id| !ids.contains(&id)));
            }
            if let Some(subcategories) = &mut genre.subcategories {
                remove_values(subcategories, &p.subcategories);
            }
        }

        let counts = &mut self
            .chapter_verses
            .get_or_insert_with(BookChapterVersesInput::default)
            .0;
        for name in patch.chapter_verses.keys() {
            let id = resolver.search(name);
            counts.retain(|other, _| match id {
                Some(id) => resolver.search(other) != Some(id),
                None => key(other) != key(name),
            });
        }
        Books::new(self.books.clone().unwrap_or_default())?;
        Ok(())
    }
}

/// By id when given, else by its name, display abbreviation, abbreviations, or OSIS id
fn find_book(books: &[Book], patch: &BookPatch) -> Option<usize> {
    if let Some(id) = patch.id {
        return books.iter().position(|b| b.id == id);
    }
    let names: Vec<String> = [&patch.book, &patch.abbreviation]
        .into_iter()
        .flatten()
        .map(|n| key(n))
        .collect();
    books.iter().position(|b| {
        let mut keys = [&b.book, &b.abbreviation]
            .into_iter()
            .chain(&b.abbreviations)
            .chain(&b.osis)
            .map(|n| key(n));
        keys.any(|k| names.contains(&k))
    })
}

fn find_genre(genres: &[GenreInput], name: &str) -> Option<usize> {
    let k = key(name);
    genres
        .iter()
        .position(|g| key(&g.title) == k || g.abbreviations.iter().any(|a| key(a) == k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::bible_data::BibleData;

    fn patch(json: &str) -> DataPatch {
        serde_json::from_str(json).unwrap()
    }

    fn data(input: BibleDataInput) -> BibleData {
        BibleData::new(input).unwrap()
    }

    #[test]
    fn merge_adds_names_and_books() {
        let mut input = BibleDataInput::defaults();
        input
            .merge(patch(
                r#"{
                    "books": [
                        { "book": "John", "abbreviations": ["jhn", "jn", "JHN"] },
                        { "id": 67, "book": "Tobit", "abbreviation": "Tob", "abbreviations": ["tb"] }
                    ],
                    "genres": [
                        { "title": "Gospels", "abbreviations": ["evangelists"], "books": ["Acts", "John"] },
                        { "title": "Apocrypha", "books": ["Tobit"] }
                    ],
                    "chapter_verses": { "Tobit": [22, 14] }
                }"#,
            ))
            .unwrap();
        let data = data(input);
        let books = data.books();
        let john = books.search("John").unwrap();
        assert_eq!(books.search("jhn"), Some(john));
        // Still matches its old names, each kept once
        assert_eq!(books.search("jn"), Some(john));
        let tobit = books.search("tb").unwrap();
        assert_eq!(books.get_name(tobit).unwrap(), "Tobit");
        let versification = data.chapter_verses().get_chapter_verses(&tobit).unwrap();
        assert_eq!(versification.get_chapter_count(), 2);
        let gospels = data.genres().genre_ids("evangelists").unwrap();
        assert_eq!(gospels.len(), 5);
        assert_eq!(data.genres().genre_ids("Apocrypha").unwrap().len(), 1);
    }

    #[test]
    fn renaming_keeps_the_old_name() {
        let mut input = BibleDataInput::defaults();
        input
            .merge(patch(r#"{ "books": [{ "book": "Song of Solomon", "abbreviations": [] }, { "id": 22, "book": "Song of Songs" }] }"#))
            .unwrap();
        let data = data(input);
        let song = data.books().search("Song of Songs").unwrap();
        assert_eq!(data.books().get_name(song).unwrap(), "Song of Songs");
        assert_eq!(data.books().search("Song of Solomon"), Some(song));
        // Genres and chapter counts that use the old name still find it
        assert!(data.chapter_verses().get_chapter_verses(&song).is_some());
    }

    #[test]
    fn merge_rejects_names_for_two_books() {
        let mut input = BibleDataInput::defaults();
        let err = input
            .merge(patch(
                r#"{ "books": [{ "book": "John", "abbreviations": ["gen"] }] }"#,
            ))
            .unwrap_err();
        assert!(err.to_string().contains("`gen`"), "{err}");
        let err = input
            .merge(patch(r#"{ "books": [{ "book": "Tobit" }] }"#))
            .unwrap_err();
        assert!(err.to_string().contains("give it an `id`"), "{err}");
    }

    #[test]
    fn remove_values_or_whole_entries() {
        let mut input = BibleDataInput::defaults();
        input
            .remove(patch(
                r#"{
                    "books": [
                        { "book": "Song of Solomon", "abbreviations": ["song"] },
                        { "book": "Jude" }
                    ],
                    "genres": [
                        { "title": "Gospels", "books": ["Jn"] },
                        { "title": "Pentateuch" }
                    ],
                    "chapter_verses": { "Genesis": [] }
                }"#,
            ))
            .unwrap();
        let data = data(input);
        let books = data.books();
        assert_eq!(books.search("song"), None);
        assert!(books.search("Song of Solomon").is_some());
        assert_eq!(books.search("Jude"), None);
        assert_eq!(data.genres().genre_ids("Gospels").unwrap().len(), 3);
        assert!(data.genres().genre_ids("Pentateuch").is_none());
        let genesis = books.search("Genesis").unwrap();
        assert!(data.chapter_verses().get_chapter_verses(&genesis).is_none());

        let mut input = BibleDataInput::defaults();
        let err = input
            .remove(patch(r#"{ "books": [{ "book": "Jhon" }] }"#))
            .unwrap_err();
        assert!(err.to_string().contains("`Jhon`"), "{err}");
    }
}

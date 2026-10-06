use std::collections::{BTreeMap, BTreeSet};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::data::books::{BookId, Books};

/// This is not guaranteed to be a valid key, I just am using a unique type
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_more::From,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct GenreKey(u32);

impl GenreKey {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genres {
    id: u32,
    genres: BTreeMap<GenreKey, Genre>,
    /// Key/Abbreviation to Genre Title
    input_to_key: BTreeMap<String, GenreKey>,
}

impl Genres {
    pub fn create(books: &Books, input: GenresInput) -> Self {
        let mut genres = BTreeMap::default();
        let mut key_to_genre = BTreeMap::default();

        let mut id = 1;
        for genre in input.0.clone() {
            // use title as key
            let ab = Self::normalize_key(&genre.title);
            let key = GenreKey::new(id);
            key_to_genre.insert(ab, key.clone());

            // use normalized abbreviations as keys
            for ab in &genre.abbreviations {
                let ab = Self::normalize_key(ab);
                key_to_genre.insert(ab, key.clone());
            }

            // search for book ids now so i only have to do it once
            let ids = if let Some(books_in_genre) = &genre.books {
                books_in_genre
                    .iter()
                    .filter_map(|b| books.search(b))
                    .collect()
            } else {
                BTreeSet::default()
            };

            let genre = Genre::new(key.clone(), ids);

            // use title as the genre key
            genres.insert(key.clone(), genre);
            id += 1;
        }

        let mut data = Self {
            id,
            genres,
            input_to_key: key_to_genre,
        };

        // In order to support using abbreviations, I should do this at the end
        for genre in input.0 {
            let Some(subcategories) = genre.subcategories else {
                continue;
            };
            for cat in subcategories {
                let Some(ids) = data.genre_ids(&cat).cloned() else {
                    continue;
                };
                if let Some(g) = data.get_mut(&genre.title) {
                    g.books.extend(ids);
                }
            }
        }

        data
    }

    /// - But this returned struct gives the user the input data; I need a separate type
    /// - Also this should return the key :/, not all the data
    pub fn search<'a>(&'a self, input: &'_ str) -> Option<&'a GenreKey> {
        let key = Self::normalize_key(input);
        self.input_to_key.get(&key)
    }

    pub fn get<'a>(&'a self, input: &'_ str) -> Option<&'a Genre> {
        let key = self.search(input)?;
        self.genres.get(key)
    }

    fn get_mut<'a>(&'a mut self, input: &'_ str) -> Option<&'a mut Genre> {
        let key = self.search(input)?.clone();
        self.genres.get_mut(&key)
    }

    pub fn genre_ids<'a>(&'a self, input: &'_ str) -> Option<&'a BTreeSet<BookId>> {
        Some(&self.get(input)?.books)
    }

    pub fn normalize_key(name: &str) -> String {
        name.to_lowercase()
            .trim()
            .trim_end_matches(".")
            .trim()
            .replace(" ", "-")
            .to_string()
    }

    pub fn base() -> &'static Self {
        &DEFAULT_GENRES
    }
}

static DEFAULT_GENRES: Lazy<Genres> = Lazy::new(Genres::default);

impl Default for Genres {
    fn default() -> Self {
        Self::create(Books::base(), GenresInput::default())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genre {
    title: GenreKey,
    books: BTreeSet<BookId>,
}

impl Genre {
    pub fn new(key: GenreKey, books: BTreeSet<BookId>) -> Self {
        Self { title: key, books }
    }
}

/**
Example:
```jsonc
[
  {
    "title": "Major Prophets"
    "abbreviations": [ "major" ],
    "books": [ "Isaiah", "Jeremiah", "Lamentations", "Ezekiel", "Daniel" ]
  },
  {
    "title": "Minor Prophets"
    "abbreviations": [ "minor" ],
    "books": [ "Hosea", "Joel", "Amos", "Obadiah", "Jonah", "Micah", "Nahum", "Habakkuk", "Zephaniah", "Haggai", "Zechariah", "Malachi" ]
  },
  {
    "title": "Prophets"
    "abbreviations": [ "pr" ],
    "subcategories": [ "Major Prophets", "Minor Prophets" ],
  },
  // ...
]
```
*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenreInput {
    title: String,
    abbreviations: Vec<String>,
    books: Option<Vec<String>>,
    subcategories: Option<Vec<String>>,
}

// static DEFAULT_GENRES_JSON: &'static str = include_str!(concat!(
//     env!("CARGO_MANIFEST_DIR"),
//     "/src/data/default_genres.json"
// ));

static DEFAULT_GENRES_JSON: &str = include_str!("./default_genres.json");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenresInput(Vec<GenreInput>);

impl Default for GenresInput {
    fn default() -> Self {
        serde_json::from_str(DEFAULT_GENRES_JSON).expect("the default genre data is valid JSON")
    }
}

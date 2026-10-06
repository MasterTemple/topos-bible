# topos-bible

Find, parse, and autocomplete Bible references in text, with exact locations.

`topos-bible` finds references like `Jn 3:16-18; 5`, `1 Cor 13:4-7, 13`, or `Ps 23` in any text
and reports where each one is: line and column (in bytes, characters, and UTF-16 code units) and
the byte range. It parses the complex forms people actually write (verse lists, chapter ranges,
ranges across chapters, `ff`, Roman numerals), checks them against each book's chapters and
verses, and avoids false positives like `is 2.5%` or `Job 1 of 3`.

It also powers [`topos`](https://crates.io/crates/topos-bible-cli), a ripgrep-style command-line
tool, and the `topos-bible` packages on [npm](https://www.npmjs.com/package/topos-bible) and
[PyPI](https://pypi.org/project/topos-bible/).

```toml
[dependencies]
topos-bible = "0.3"
```

## Search text

```rust
use topos_bible::matcher::BibleMatcher;

let matcher = BibleMatcher::default();
let text = "Read Jn 3:16-18; 5 and then\nRom 8:28.";
let found: Vec<_> = matcher.search(text);

let books = matcher.data().books();
let first = &found[0];
assert_eq!(books.get_name(first.psg.book).unwrap(), "John");
assert_eq!(first.psg.segments.to_string(), "3:16-18; 5");
assert_eq!(&text[first.location.bytes.start..first.location.bytes.end], "Jn 3:16-18; 5");

// Lines and columns are 1-based; columns come in bytes, characters, and UTF-16 code units
let second = &found[1];
assert_eq!((second.location.start.line, second.location.start.column), (2, 1));
```

`John 1:1, 3 John 5` is `John 1:1` and `3 John 5`: a number before a book name belongs to the book.

## Write references

```rust
use topos_bible::{
    matcher::BibleMatcher,
    segments::formatter::{BookStyle, FormatOptions},
};

let matcher = BibleMatcher::default();
let data = matcher.data();
let passage = matcher.find("see 1 cor 13:4-7,13").unwrap().psg;

let name = FormatOptions::default();
assert_eq!(name.passage(&passage, data).unwrap(), "1 Corinthians 13:4-7,13");

let short = FormatOptions { book: BookStyle::Abbreviation, ..FormatOptions::default() };
assert_eq!(short.passage(&passage, data).unwrap(), "1 Cor 13:4-7,13");

// OSIS, and back
let osis = passage.to_osis(data.books()).unwrap();
assert_eq!(osis, "1Cor.13.4-1Cor.13.7 1Cor.13.13");
assert_eq!(data.books().parse_osis(&osis).unwrap(), passage);
```

## Autocomplete

Completions come from the same grammar as search, so they always agree with it. Each one is a
`TextEdit` with a byte range, ready for an editor or a language server:

```rust
use topos_bible::{matcher::BibleMatcher, segments::autocomplete::CompleteOptions};

let matcher = BibleMatcher::default();
let text = "Read John 3:";
let completions = matcher.complete(text, text.len(), &CompleteOptions::default());
assert_eq!(completions.len(), 36); // every verse of John 3
assert_eq!(completions[15].label, "John 3:16");

// Apply one
let edit = &completions[15].edit;
let mut done = text.to_string();
done.replace_range(edit.range.clone(), &edit.text);
assert_eq!(done, "Read John 3:16");
```

## Filters

Keep references by testament, genre, book, or passage. Testaments narrow the rest, included
genres and books add up, and exclusions always win:

```rust
use topos_bible::filter::{
    bible_filter::BibleFilter,
    filters::{genre::GenreFilter, testament::TestamentFilter},
};

let mut filter = BibleFilter::default();
filter.include(TestamentFilter::New)?;
filter.include(GenreFilter::new("Pauline Epistles"))?;
filter.filter_outside("Romans 9-11")?;
let matcher = filter.create_matcher();

let found = matcher.search("John 3:16, Rom 8:28, Rom 9:2, Gen 1:1");
let books = matcher.data().books();
let names: Vec<_> = found.iter().map(|m| books.get_name(m.psg.book).unwrap().as_str()).collect();
assert_eq!(names, ["Romans"]);

// Contradictory filters say why nothing can match
let mut filter = BibleFilter::default();
filter.include(TestamentFilter::Old)?;
filter.include(GenreFilter::new("Pauline Epistles"))?;
assert!(filter.contradiction().is_some());
# Ok::<(), topos_bible::error::ToposError>(())
```

## Problems

References that parse but don't exist:

```rust
use topos_bible::matcher::BibleMatcher;

let matcher = BibleMatcher::default();
let problems = matcher.problems("John 3:16, 4:99");
assert_eq!(problems.len(), 1);
assert_eq!(problems[0].bytes, 11..15);
```

## Book context

In notes about one book, bare references like `3:16` can count too:

```rust
use topos_bible::matcher::{BibleMatcher, context::BookContext};

let matcher = BibleMatcher::default();
let john = matcher.data().books().search("John").unwrap();
let matcher = matcher.with_context(BookContext::Book(john));
let found = matcher.search("As 3:16 says");
assert_eq!(found[0].psg.book, john);
```

`BookContext::headings` instead takes the book from headings, like `^#+ {book}$` in Markdown.

## Custom data

The books (names, abbreviations, OSIS ids), genres, and chapter and verse counts are data. Replace
any part of them, or merge into and remove from the defaults:

```rust
use topos_bible::data::{bible_data::{BibleData, BibleDataInput}, patch::DataPatch};
use topos_bible::{filter::bible_filter::BibleFilter, matcher::BibleMatcher};

let mut input = BibleDataInput::defaults();
let more: DataPatch = serde_json::from_str(
    r#"{ "books": [{ "book": "John", "abbreviations": ["jhn"] }] }"#,
)?;
input.merge(more)?;
let fewer: DataPatch = serde_json::from_str(r#"{ "books": [{ "book": "Jude" }] }"#)?;
input.remove(fewer)?;

let matcher = BibleFilter::new(BibleData::new(input)?).create_matcher();
assert_eq!(matcher.search("Jhn 3:16").len(), 1);
assert_eq!(matcher.search("Jude 5").len(), 0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Every name must mean exactly one book, so merging an abbreviation that another book already uses
is an error.

## Formats

[`topos-bible-formats`](https://crates.io/crates/topos-bible-formats) finds references in HTML
(with text-fragment links), SRT/WebVTT/SBV subtitles (with timestamps), EPUB (with CFIs), JSON,
XML, and PDF (with page numbers).

## License

CC0-1.0

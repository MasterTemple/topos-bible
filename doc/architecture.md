# Target Architecture

The design that the [roadmap](../ROADMAP.md) works toward. The goals are one grammar, clear pipeline stages, a small core that builds for WASM and FFI, and format support as add-ons.

## Crates

```
crates/
  topos/           core: data, lexer/parser, resolver, matcher, autocomplete, formatting, OSIS
                   deps: regex or aho-corasick, serde, thiserror (nothing native)
  topos-formats/   format adapters behind features: html, srt, vtt, epub, pdf (mupdf), json
  topos-cli/       ripgrep-style binary (depends on topos + topos-formats)
  topos-ffi/       one BoltFFI façade over `topos` (TS/WASM, Swift, Kotlin)
  topos-py/        PyO3, only if BoltFFI can't target Python; a thin wrapper over the same façade
```

Delete `topos-parser`, `topos-pdf` (fold it into `topos-formats/pdf`) and `topos-ts` (replaced by `topos-ffi`).

## Pipeline

```
text ──► 1. find books ──► 2. parse segments ──► 3. resolve ──► 4. validate ──► 5. filter ──► 6. locate
         (all books)       (CST + spans)         (Passage)       (versification)  (BookId,      (format-specific
                                                                  + confidence     inside/out)    location)
```

1. **Find books.** Match every book name and abbreviation, always, case-insensitively and leftmost-longest. Either sort the alternation longest-first and `regex::escape` each key, or use `aho-corasick` with `MatchKind::LeftmostLongest`. A candidate only counts if a number follows it. Filtering never happens here, which fixes #8.
2. **Parse segments.** Parse the window from the end of the book match up to the start of the next candidate. Keep the current window rule: it's what makes `John 1:1, 3 John 5` work, because the next book match (`3 John`) ends the window for `John`. Parse as much as fits and stop. The output is a **lossless syntax tree**: every number, delimiter and whitespace run with its byte span. Search, autocomplete and the formatter all consume this one tree.
3. **Resolve.** Turn the syntax tree into a `Passage`, using book data and a `Context` carried along the document. This is where every interpretation rule lives:
   - Bare numbers are chapters until a verse has been seen (`John 1, 3` → chapters 1 and 3).
   - Single-chapter books treat a bare number as a verse (`Jude 5` → 1:5).
   - `ff`, `f.`, `cf.`, `v.`/`vv.` and `ch.` are handled here (#10).
   - A bare `v. 4` or `ch. 3` refers to the current book or chapter in the `Context`, for document-level context (#6).
4. **Validate.** Check chapters and verses against the versification (`chapter_verses`). Either drop impossible references or clamp them and flag them. Give each match a `confidence` from ambiguity signals: ambiguous abbreviations (`is`, `am`, `job`, `mark`), no `:`, Roman numerals, out-of-range numbers. Search mode applies a threshold. Explicit `parse()` accepts anything that parses (#3).
5. **Filter.** Apply the book set and inside/outside passages by `BookId` and overlap.
6. **Locate.** Map the byte span to a location type (see Formats).

## Grammar: one parser

The segment grammar is small. A sketch:

```
segments  := segment (SEP segment)*
segment   := num [CH num] [RANGE num [CH num]]
num       := DIGITS [SUBVERSE] | ROMAN        (ROMAN only in chapter position)
SEP := , ;      CH := : .      RANGE := - – — ‒ − ⸺      SUBVERSE := a b c d
```

**Recommendation:** use a hand-written lexer (tokens carry spans) and a small hand-written recursive-descent parser instead of chumsky. Reasons:
- The grammar is regular, so there's little for a combinator library to buy.
- Autocomplete needs "what token is expected next" for a partial input. A hand-written parser gives you that for free as its state at end of input.
- The chumsky and `from_nested_tuple` lifetime and type complexity is what currently breaks the build, and it adds compile time.

If you would rather keep chumsky, the important part is still to have **one** grammar and one tree, built once and stored in a `static`.

## Core types

```rust
pub struct BookId(u8);
pub struct Chapter(u8);
pub struct Verse(u8);                     // newtypes stop chapter and verse being swapped

pub enum Segment { … }                    // keep the 6 variants; implement `Ord` by hand
pub struct Passage { book: BookId, segments: Segments }

pub struct Match {
    pub passage: Passage,
    pub span: Range<usize>,               // byte offsets into the searched text: the single source of truth
    pub confidence: f32,
}
```

Line and column are derived from the span on demand by a `LineIndex` that offers byte, char and UTF-16 columns (LSP needs UTF-16).

## Data

Extend `default_books.json` with:
- `osis`: `"Gen"`, `"1Sam"`, `"John"`, …, for interop.
- `ambiguous: ["is", "am", "job", …]`: abbreviations that are also common words.

Keep versification (`chapter_verses`) as its own swappable dataset, with KJV as the default. Later, `locale` groups book names per language.

## Formats (`topos-formats`)

Every format is an adapter with the same shape, which HTML, PDF and SRT already follow informally:

```rust
pub trait SourceFormat {
    type Location;
    fn text(&self) -> &str;                                 // plain text that the matcher searches
    fn locate(&self, span: Range<usize>) -> Self::Location; // map a byte span back to the source
}
```

| Format | Location |
|---|---|
| Plain text | line and column (byte, char, UTF-16) |
| HTML | text fragment and selection |
| SRT / VTT | cue id and timestamps (cues are multi-line) |
| PDF | page and rects |
| EPUB | CFI |
| JSON / XML | JSON pointer or XPath, plus an offset within the string |

## Autocomplete

```rust
pub struct CompleteOptions {
    pub expand_abbreviations: bool,        // "Gen" → "Genesis"
    pub delimiter_spacing: Spacing,        // "1:1,3" vs "1:1, 3"
    pub normalize_dashes: bool,
    pub max_results: Option<usize>,
}

pub struct Completion {
    pub label: String,                     // what a menu shows: "Genesis 1:1-3"
    pub kind: CompletionKind,              // Book | Chapter | Verse | Delimiter
    pub edits: Vec<TextEdit>,              // byte ranges + replacement text (LSP-ready)
}

impl Topos {
    pub fn completions(&self, text: &str, cursor: usize, opts: &CompleteOptions)
        -> impl Iterator<Item = Completion> + '_;
}
```

- Parse the reference that ends at `cursor`. The parser's state at end of input says what comes next: a book (prefix-match the names), a chapter, a verse after `:`, or a range end. Versification supplies the valid numbers.
- Normalization (abbreviation → full name, spacing, dashes) is the **formatter** applied to the same syntax tree, sent as extra `TextEdit`s. `formatter/options.rs` and autocomplete then share one implementation.
- A simple text box applies `edits` to get the new string. An LSP server maps each `TextEdit` to a UTF-16 range through `LineIndex`.

## Interop

- `Passage::to_osis()` / `Passage::from_osis()`: `John.3.16-John.3.18`, with `John.3` for a whole chapter.
- An integer BCV key (`43003016`) for sorting, database keys and range queries.
- Make OSIS the canonical serialized form. This is roadmap item 11, "a standard format for segment ranges": the `Display`/`FromStr` strings stay for humans.
- Later: USFM/Paratext book codes (`JHN 3:16`), which only need another column in the book data.

## FFI façade

Bindings only see plain, owned types and one opaque object:

```rust
#[export] pub struct Topos { … }             // owns BibleData + options; built once
impl Topos {
    pub fn new(config_json: Option<String>) -> Result<Topos, ToposError>;
    pub fn search(&self, text: String) -> Vec<MatchDto>;
    pub fn parse(&self, reference: String) -> Option<PassageDto>;
    pub fn complete(&self, text: String, cursor: u32, opts: CompleteOptionsDto) -> Vec<CompletionDto>;
    pub fn format(&self, passage: PassageDto, style: FormatStyle) -> String;
}
```

The façade has no generics, no lifetimes and no `String` errors. One `ToposError` enum (built with `thiserror`) is used across the core.

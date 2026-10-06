# Roadmap

The phases are in order, and each one leaves the repo working.
- Findings and file references: [doc/review.md](./doc/review.md)
- Target design: [doc/architecture.md](./doc/architecture.md)

## Phase 0: Get back to green

- [x] Make the `pdf` feature opt-in, and gate the `PDFMatchError` import and variant
- [x] Replace `.from_tuple()` calls with `.map(...)`, or fix `from_nested_tuple`
- [ ] Pin the `htmloc` git rev (the `Selection { bytes }` change is fixed)
- [x] Fix the CLI and binding call sites for the `MatchResult` return type
- [ ] Delete dead code: `crates/topos-parser` and the placeholder exports (the old segment parsers and unlinked files are already gone)
- [x] Replace the absolute-path HTML test fixture with a file in the repo (the test is `#[ignore]`d because `htmloc` takes minutes on it)
- [ ] Add CI: `test` + `clippy` for both default features and `--no-default-features`

## Phase 1: Correctness, with a test corpus first

- [x] Add `tests/cases/search.txt`, a table of `input => expected references`, run by one test
- [x] Fix the `5:12-6:6` resolution bug
- [x] Restore the "chapters until a verse is seen" rule (and `;` starts chapters again)
- [x] Handle single-chapter books (`Jude 5`, `3 John 5`)
- [x] Always match all books and filter by `BookId` afterward: Searching `John` only matches `1 John` [#8](https://github.com/MasterTemple/topos/issues/8)
- [x] Escape regex keys, sort them longest-first, and drop the hard-coded `1..=66` (plus ASCII word boundaries: search is 31x faster)
- [x] Implement `Ord` for `Segment` by hand, and fix `Passage::contains` / `Segments::fully_contains`
- [x] Restrict Roman numerals to chapter position, whole words, and canonical forms; use `is_ascii_digit`
- [x] Validate against versification (Validate match segments based on actual verse in the text)
- [x] Add a fuzz target (`cargo fuzz run search` in `fuzz/`), plus a seeded randomized test of the same invariants in CI

## Phase 2: One parser and a resolve stage

- [x] One lexer/parser that produces a lossless syntax tree with spans, replacing minimal, verbose and the autocomplete regexes (`segments/grammar`)
- [x] A resolver stage (`segments::resolve`) and document context (`matcher::context`)
  - [x] Parse older formats / Roman numerals / sub-verses: `Matth. x, 8` [#5](https://github.com/MasterTemple/topos/issues/5)
  - [x] `cf` and `ff` [#10](https://github.com/MasterTemple/topos/issues/10)
  - [x] Support dashes in book names for justified text [#11](https://github.com/MasterTemple/topos/issues/11)
  - [x] Contextual parsing: verse headings in a document about one book [#6](https://github.com/MasterTemple/topos/issues/6)
- [x] Rules plus `ambiguous` abbreviations in the book data (instead of a numeric confidence score): Reduce false positives (`"is"` for Isaiah) [#3](https://github.com/MasterTemple/topos/issues/3)
- [x] One `ToposError` enum, and no `unwrap` in library code (except the PDF module, rewritten in Phase 3)
- [x] Byte spans as the source of truth, with a `LineIndex` that gives byte, char and UTF-16 columns

## Phase 3: Crate split and formats

- [ ] Split `topos` (core) from `topos-formats` (feature-gated adapters behind a `SourceFormat` trait)
- [ ] Generalize match location [#9](https://github.com/MasterTemple/topos/issues/9): plain text, HTML, SRT (multi-line cues), VTT, PDF (merge `topos-pdf`), EPUB, JSON, XML

## Phase 4: Formatting, interop, autocomplete

- [ ] Formatter driven by the syntax tree: Provide user-specified formatting options [#4](https://github.com/MasterTemple/topos/issues/4)
- [ ] OSIS parse and format, plus a BCV integer key: Create a standard format for specifying segment ranges
- [ ] Autocomplete `completions(text, cursor, opts)` that returns `TextEdit`s: books, chapters, verses, and normalization edits

## Phase 5: CLI

- [ ] Stream results while the directory walk runs (no `thread::scope` blocking)
- [ ] Output modes: rg-style `path:line:col: ref` (default when piped), grouped and colored (TTY), real NDJSON `--json`, `--count`
- [ ] Context: `-C/-A/-B` lines, `--only-matching`, `--format osis|full|abbrev`, highlighting
- [ ] Choose a format by file extension through `topos-formats`, and report unreadable files on stderr
- [ ] Clarify inputs: `topos [PASSAGE] [PATH]...`, where the optional passage acts as `--inside`. Use `--text`/stdin for literal text
- [ ] `--config` for custom book, genre and versification data
- [ ] Cache search results [#2](https://github.com/MasterTemple/topos/issues/2)

## Phase 6: Bindings and tooling

- [ ] A `topos-ffi` façade (BoltFFI) with owned DTOs; remove `topos-ts`
- [ ] Python through BoltFFI if it's supported, otherwise a thin PyO3 wrapper over the same façade
- [ ] An LSP server (diagnostics for invalid references, completion, hover with the normalized reference) built on the autocomplete API
- [ ] Incremental parsing (re-scan only the edited lines; easy once matches are span-based)
- [ ] Criterion benchmarks on a large corpus

# topos

Finds and parses Bible references in text and documents, reporting where each one is in the source's own terms. See `ROADMAP.md` (what's done and planned) and `doc/architecture.md` (design). `doc/review.md` is a historical snapshot; most of it is fixed.

## Crates

- `topos-lib` (core, no native deps). Pipeline: book candidates → `segments::grammar` (lexer and parser that never fails, lossless `SegmentList` with byte spans) → `segments::resolve` (what numbers mean, plus versification checks) → filter → `LineColLocation`.
  - `BibleMatcher::search(text)`, `complete(text, cursor, opts)` (returns `TextEdit`s), `problems(text)` (references that don't exist)
  - `FormatOptions` (formatting), `segments::osis` (OSIS and BCV keys), `matcher::context` (bare `1:1` references with a book context)
- `topos-formats`: the `Format` trait / `search_format::<L>()` for html, srt (also WebVTT and SBV), epub, json, xml, and pdf (feature `pdf`, MuPDF 0.8).
- `topos-cli`: binary `topos`. Paths are positional; text comes from `--text` or piped stdin. Default options come from `~/.config/topos/config.toml` (`--config PATH` reads another file, `--no-config` skips it). `-i` keeps references inside a passage, `-o` keeps overlapping ones. `--data` takes custom Bible data JSON. `--cache` is opt-in and stores unfiltered results (one postcard entry per file, with a bitset of the books it mentions), filtered per search with `BibleMatcher::keeps`; `tests/unfiltered.rs` checks that equals a filtered search.
- `topos-lsp`: completion, hover, document symbols, diagnostics (`lsp-server` 0.10).
- `obsidian/`: an Obsidian plugin (TypeScript, React sidebar) built on the `topos-bible` npm package, with the WebAssembly embedded via esbuild aliases. `src/core/` has no Obsidian imports and is tested with `node --test`; `test/plugin.test.ts` loads the built `main.js` against a stand-in for Obsidian's API. The plan is in `doc/obsidian-plugin.md`.
- `topos-ffi`: the BoltFFI 0.31 façade, the only bindings crate. Offsets are in the caller's unit (byte, char, UTF-16). `boltffi.toml` was made with `boltffi init`. Build with `boltffi pack python`, or `npm install && npm run pack:wasm` for WASM. Its README and `examples/` (Node, Python) are verified; the Swift and Kotlin snippets are not.

## Commands

- `cargo test`. CI also runs `cargo fmt --check`, `cargo clippy --all-targets` with `-D warnings` on stable, and `cargo test -p topos-formats --features pdf`.
- `cargo bench -p topos-lib` (Criterion), `cd fuzz && cargo +nightly fuzz run search`.
- Use `rg` and `fd`, not `grep` and `find`.

## Conventions and lessons

- Add search behavior changes to `crates/topos-lib/tests/cases/search.txt` (`input => Ref | Ref`; `?` marks a known failure; `\n` is a line break). Expected values are `Segments`' display form, which is designed to parse back to the same passage.
- The interpretation rules live only in the resolver: chapters until a verse appears, `;` resets to chapters, single-chapter books, `ff`, Roman numerals only where a chapter goes, and a `.`-part fallback.
- False-positive rules (book candidates, `FoundPassage::find`): candidates don't consume the chapter, ambiguous abbreviations (marked `ambiguous` in `default_books.json`) need an explicit verse, there are no candidates inside runs of 64+ characters without whitespace, a book glued to its chapter needs a verse, and the book name's casing must be plausible. Test new rules against a real corpus (diff old and new output), not only unit tests.
- Quantities aren't references: a reference that runs into `5%`, `39pt` or `1.5°` is dropped. Ambiguous abbreviations need a `:` verse (or a `.` verse after `Is.`).
- Performance: keep the candidate regex on `find_iter` with ASCII `(?-u:\b)`. Capture groups or Unicode `\b` made it about 30× slower.
- Line and column positions are 1-based, with byte, char, and UTF-16 columns (`LineIndex`). Byte offsets are the source of truth.
- Errors: `ToposError` in the core, `FormatError` in formats. No `unwrap` in library code.
- `Cargo.lock` is gitignored. Commit messages end with a `Co-Authored-By` line.

# Codebase Review (2026-10)

What's wrong right now, ranked by priority. The fixes are planned in [ROADMAP.md](../ROADMAP.md), and the target design is in [architecture.md](./architecture.md).

## 1. The workspace does not build

`cargo build -p topos-lib` fails today, both with and without default features:

| Cause | Where | Fix |
|---|---|---|
| `mupdf-sys` 0.4.4 fails to compile (it's the `pdf` feature, which is on by default) | `topos-lib/Cargo.toml` `default = ["pdf"]` | Make `pdf` opt-in, and move format adapters out of core (see architecture) |
| `pdf::PDFMatchError` is imported without a feature gate | `matcher/matcher.rs:13` | Gate the import and the `MatchError::PDF` variant with `#[cfg(feature = "pdf")]` |
| `.from_tuple()` with chumsky 0.12 causes "temporary value dropped while borrowed" | `parser/verbose/mod.rs:58-81`, `parser/verbose/components.rs:83`, `location/srt.rs:65,78` | Replace with plain `.map(\|((a, b), c)\| …)`, or fix `from_nested_tuple`. The published 0.1.2 may not match your local copy |
| `htmloc` is an unpinned git dependency, and `Selection` gained a `bytes` field | `location/html.rs:28` | Pin `rev = "…"` in the workspace `Cargo.toml` |
| The CLI expects `search()` to return a `Vec`, but it now returns a `MatchResult` | `topos-cli/src/matches.rs:25` | Update the call site |

Other repo hygiene:
- `tests/html/mod.rs` uses `include_str!` on an absolute path in `~/Downloads`, so it can't build on any other machine.
- There's no CI. Add one job that runs `cargo test --workspace` and `cargo clippy` for both default and `--no-default-features`.

## 2. Correctness bugs

### Segment resolution (`segments/segments.rs`, `From<MinimalSegments>`)
- **`5:7-9,12-6:6` resolves to `12:1-6:6`.** The "no explicit start verse, end has a verse" branch (lines 149-155) uses `seg.start` as a chapter even when a previous segment exists. It should be `chapter_range(prev.ending_chapter(), seg.start, …)`. The old, now commented-out test in `parse.rs` expected `5:12-6:6`.
- **Bare numbers after a full chapter become verses.** `John 1, 3` becomes `John 1:3`, and `John 1-2, 4-5` becomes `John 1-2, 2:4-5`. The old parser had `check_for_full_chapters`, and that rule was lost in the chumsky port.
- **Single-chapter books aren't handled.** `Jude 5` and `3 John 5` resolve to chapter 5. The helpers `FullChapter::as_single_chapter_book_verse` and `ChapterVerses::has_one_chapter` exist but are never called.
- **There's no range validation** against `chapter_verses`, so `Mar 25, 2025` becomes Mark 25 (which has only 16 chapters).

### Comparison and containment
- `Segment` derives `Ord` but hand-writes `PartialOrd` with different semantics (`segment.rs:34,59`). `sort()` and `<` disagree. Implement `Ord` by hand and derive `PartialOrd` from it.
- `Passage::contains` just calls overlap (`segments.rs:34`).
- `Segments::fully_contains` (`segments.rs:64`) checks the inverse direction and uses `any` where it needs "every segment of `other` is covered".

### Book matching
- **Issue #8 (`-b John` matches the `John` inside `1 John`):** the cause is that the filter builds a regex containing only the included books (`filter.rs:113`). Without `1 john` in the alternation, `\bjohn` matches inside "1 John". The fix is to always match every book, then filter by `BookId`.
- Book keys are joined into a regex without `regex::escape` (`books.rs:98`, `filter.rs:114`, `data.rs:30`). Any user data containing `.`, `(` or `+` will break or misbehave.
- **Performance:** the book regex is the search bottleneck. It took about 420 ms on 274 KB of text in a release build, while parsing all segments took 0.07 ms. A case-insensitive alternation of hundreds of keys with Unicode `\b` is slow in `regex`. Prefer `aho-corasick` (see architecture) or an ASCII-only word boundary.
- `BibleFilter::new` hard-codes `1..=66` (`filter.rs:51`), so custom data with more books (such as the Apocrypha) can never be matched.

### Parser details
- **Roman numerals are accepted anywhere, in any case.** In `John 3:16, I think`, the `I` parses as verse 1, and `civil` is all Roman-numeral letters (`parser/minimal.rs:42`). Restrict them to chapter position, require a word boundary after them, and reject non-canonical forms like `IIII` and `VX`.
- `Decimal` uses `char::is_numeric` (`parser/components.rs:9`), which accepts non-ASCII digits that `u8::from_str` then rejects. Use `is_ascii_digit`.
- `Lengthed::from_span` casts lengths with `as u8` (`parser/verbose/len.rs:25`). Whitespace runs over 255 bytes wrap silently and corrupt spans.
- The range-dash set duplicates `—` and is missing U+2012 (‒) and U+2212 (−).
- `MinimalSegments::parse` rebuilds the chumsky parser on every call (`minimal.rs:31`).

### Locations
- **SRT:** a cue may contain only one text line (`srt.rs:83-91`). One multi-line cue makes the whole document fail to parse, so the entire search returns `Err`.
- Columns come from `line-col` and are char-based. LSP needs UTF-16 columns, and editors often want byte offsets. Keep byte offsets as the source of truth and convert on demand.

### CLI (`topos-cli`)
- `--mode json` prints `{ "type": "match" }` with no match data (`outputs.rs`).
- `std::thread::scope` blocks until the whole directory walk finishes, so nothing streams. The `Count` timer measures the wrong thing as a result (`inputs.rs:88`).
- Unreadable or binary files are silently dropped by `.filter_map(Result::ok)`.
- `--config`, `-v`, `-c`, `--before` and `--after` are parsed but do nothing.
- The README says inclusion and exclusion order matters. It doesn't: `TryFrom<Args>` applies every include, then every exclude.
- The positional argument means "a path if one exists, otherwise text", so `topos John` behaves differently depending on whether `./John` exists.

## 3. Design issues

1. **Too many parsers for one grammar.** Segment syntax is implemented four times: the chumsky "minimal" parser, the chumsky "verbose" parser, the regexes in `autocomplete/full.rs` and `incomplete.rs`, and the old hand parser in `segments/parse.rs` (fully commented out). Each one has different whitespace, Roman-numeral and sub-verse rules, so search, autocomplete and formatting disagree.
2. **Parsing and interpretation are mixed together.** `From<MinimalSegments>` decides what a bare number means using only the previous segment. Rules like full chapter vs. verse, single-chapter books, `ff`, `v.`/`vv.` and document context all need a separate resolve step with access to versification data.
3. **Filtering is done in the regex.** It's a premature optimization, and it causes #8.
4. **Format adapters are welded to core.** `mupdf`, `epub`, `roxmltree` and `htmloc` are non-optional dependencies of `topos-lib`. This makes WASM and FFI builds heavy or impossible, and there's a second PDF implementation in `topos-pdf`.
5. **The public API isn't FFI-shaped.** Generic `Matcher::Input<'a>`, borrowed `InputAutoCompleter<'a>`, `String` errors and `Box<dyn Error>` (not `Send + Sync`) don't map onto BoltFFI, PyO3 or wasm-bindgen.
6. **Errors are a mix of `String`, `Option`, `AnyResult` and `unwrap`.** Examples: `InputAutoCompleter::new` and the bindings' `get_name(..).unwrap()`. `filter_inside` silently ignores a passage it can't parse.
7. **Dead code.** `crates/topos-parser`, `segments/parse.rs`, the `autocomplete/{completer,joiner,segment,suggestion}.rs` files (not in the module tree), `formatter/{token,context}.rs` (fully commented out), `AnyLocation`, and the `sum_as_string` and `distance` placeholder exports.
8. **Primitive types for chapters and verses.** Bare `u8` makes it easy to swap chapter and verse. Newtypes `Chapter(u8)` and `Verse(u8)` cost nothing and catch that.

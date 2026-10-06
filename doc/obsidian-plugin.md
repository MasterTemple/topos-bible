# Obsidian Plugin Plan

An Obsidian plugin (`obsidian/`) that brings everything topos-bible does into the vault: the
CLI's search and filters as a sidebar, plus editor features the CLI can't have. It uses the
`topos-bible` npm package (Rust compiled to WebAssembly) and React for the sidebar.

## Features

### Verse search sidebar (the CLI as a GUI)

| CLI | Sidebar |
|---|---|
| `topos [PATH]...` | Scope: whole vault, a folder, or the current file |
| `-t/--nt/--ot`, `-g`, `-b`, `--exclude-*` | Testament, genre, and book filters (chips with autocomplete), each include or exclude |
| `-i` / `-o` / `--any-overlap` / `--exact-overlap` / `--exclude-overlap` | Passage filters: inside, names its verses, any shared verse, exactly, or no shared verse (inputs with reference autocomplete) |
| `-m grouped`, `-C` | Results grouped by file, each with its line of context and the reference highlighted |
| `-m count` / `--total-count` | Counts per file and in total |
| `-f name\|abbreviation\|osis` | Reference style |
| `--sort` | Sort by file, by position in the Bible, or group by book |
| jumping in Vim's quickfix list | Click a result to open the note with the reference selected |

Filters update the results live. Included testaments narrow the included genres and books,
which add up; exclusions always win (the same rules as the CLI, which warns about contradictory
inclusions like `--ot -g "Pauline Epistles"`; the sidebar shows the warning even with the filter
panel closed). Searches can be saved under a name, stored as CLI options (an optional folder plus
`-t/--nt/--ot`, `-g`, `-b`, `--exclude-*`, and the passage filters), and edited in the settings.
The sort order and grouping are remembered. Results can also be sorted canonically (by book, chapter, and
verse) or grouped by book, which the CLI can't do.

### Editor

- **Autocomplete while typing**: after a book name, suggest chapters, then verses, then range
  ends (`John 3:` → `John 3:1` … `John 3:36`). Book names complete too (`1 Co` →
  `1 Corinthians`); in prose that would fire on every word, so by default book names only
  complete when they start with a capital letter or a number.
- **Clickable references**: references in the editor (live preview and source) and in reading
  view are underlined. Clicking (Ctrl/Cmd-click in the editor, so editing still works) opens
  the verse in Literal Word.
- **Context menu** on a reference: open in Literal Word, copy as OSIS, find it in the vault.

### Commands

- Open verse search
- Search references in the current file
- Find references overlapping the one under the cursor (opens the sidebar with that filter)
- Open the reference under the cursor in Literal Word
- Copy the reference under the cursor as OSIS
- Insert a verse reference (a dialog with autocomplete)
- Go to a reference: type a passage, pick one of the vault's matching references, jump there
- Normalize references in the selection or note to the chosen style (`jn 3:16` → `John 3:16`)

### Every input autocompletes

The sidebar's passage filters, the book filter, and the insert and go-to dialogs all use the
same reference autocomplete as the editor.

### Literal Word

Deep links are `https://app.literalword.com/[translation/]<book 1-66>/<chapter>[/<verse>]`
([docs](https://app.literalword.com/deep-links)). Literal Word doesn't support ranges, so a
reference opens at its first verse, or at the chapter for whole chapters. The translation
(NASB, LSB, ESV, NKJV, KJV, or Literal Word's default) is a setting.

### Settings

Reference style; Literal Word translation; whether editor clicks need Ctrl/Cmd; underline
references in the editor and reading view; autocomplete on/off, book-name completion
(off, capitalized, always), and how many suggestions; files and folders to exclude from search.

## Design

- **One engine instance**: the WASM module is embedded in `main.js` and initialized once on load
  (no network, works on mobile).
- **Offsets**: everything uses UTF-16 offsets (`OffsetUnit.Utf16`), which is what CodeMirror
  and JavaScript strings use, so matches map straight to editor positions.
- **Pure logic is separate from Obsidian** (`src/core/`: filters, Literal Word links, search
  results, sorting), so it is unit tested with Node; the Obsidian glue stays thin.
- **Editor decorations** only search the visible lines and update as you scroll or type.
- **Indexing never blocks the editor**: the vault is searched in a Web Worker with its own copy of
  the engine (the worker's code is bundled separately and embedded in `main.js`; the WebAssembly
  is sent to it at startup rather than embedded twice), in batches of up to 200 files or 4M
  characters. If workers are unavailable it falls back to the main thread, yielding between
  files. The sidebar re-renders at most twice a second while results arrive.
- **Optional CLI engine (desktop)**: runs `topos . --no-config -m json [--cache]` in the vault
  folder and streams the results into the index. The CLI's JSON includes UTF-16 offsets, the
  book id, segments, and the line text, so its hits are identical to the built-in engine's
  (tested). Edits during a reindex win over the reindex's older results.

## Library additions

The bindings gained what the filters need: `books()`, `genres()`, `find_book()`, and
`find_genre()` (on top of `search`, `parse`, `complete`, `contains`, `overlaps`, `verses`, and
`verse_ranges`). Until a release includes them, the plugin builds against the local package in
`crates/topos-ffi/dist/wasm/pkg`.

## Not in the first version

- PDF and EPUB search inside the vault (needs the native formats crate)
- Book context for notes about one book (`--context-book`)
- Diagnostics for references that don't exist (needs `problems` in the bindings)

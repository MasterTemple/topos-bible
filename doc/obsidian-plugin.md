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
  view (callouts, tables, and embeds too) are marked with a soft glow and a dashed underline in
  the accent color (or a chosen one), not as ordinary links. Ctrl/Cmd-click on desktop (a tap on
  mobile outside the editor, or in it if a setting allows and the editor isn't focused) opens its link (Literal Word by default; see Links).
- **Context menu** on a reference: open its link, copy as OSIS, find it in the vault.

### Commands

- Open verse search
- Search references in the current file
- Find references overlapping the one under the cursor (opens the sidebar with that filter)
- Open the reference under the cursor in the browser
- Copy the reference under the cursor as OSIS
- Insert a verse reference (a dialog with autocomplete)
- Go to a reference: type a passage, pick one of the vault's matching references, jump there
- Normalize references in the selection or note to the chosen style (`jn 3:16` → `John 3:16`)

### Every input autocompletes

The sidebar's passage filters, the book filter, and the insert and go-to dialogs all use the
same reference autocomplete as the editor.

### Links

A link template (`src/core/links.ts`) turns a reference into an address:
`https://biblehub.com/{book.biblehub}/{chapter}[-{verse}].htm`. Placeholders are the book's
name, abbreviation, OSIS id, number, USFM code, and BibleHub name, the first chapter and verse
(and the end of the first range), and the whole reference written out or as OSIS; filters
(`{book|lower|kebab}`) change them, and a `[ ]` part is left out when a placeholder in it has no
value. Literal Word ([deep links](https://app.literalword.com/deep-links)), BibleHub,
lets.bible, BibleGateway, and YouVersion are built in; links can also be turned off. Most sites open one verse
or chapter, so a reference opens at its first verse. The plugin README lists the placeholders.
Settings from before templates (a Literal Word translation) become the matching template.

### Settings

Reference style; where references open (a site, a link template, or nowhere); whether editor clicks need Ctrl/Cmd; whether a tap opens references in the editor on mobile; highlight
references in the editor and reading view; autocomplete on/off, book-name completion
(off, capitalized, always), and how many suggestions; files and folders to exclude from search;
whether to search EPUBs (and on mobile, whether to search books no other device has indexed);
the reference index (its size, and **Rebuild**).

### EPUBs (with EPUB++)

EPUB++ (a separate plugin) opens EPUBs in Obsidian and has an API for annotation providers. With
**Search EPUB files** on, books the index doesn't have are searched in one of two ways:

- With the CLI engine (desktop), `topos -m index -- <book>...` searches them, named on the command
  line (in batches that fit on one) so ignore files can't skip them, and in parallel off
  Obsidian's thread. Each entry has the CFIs, spine indexes, chapters, and offsets through the
  book's text (`topos-bible-formats` extracts text and writes CFIs exactly as EPUB++ does, checked
  on 98 books), so the references are the same as the other way's. Books it can't read, a CLI
  that fails, or one too old for `-m index` fall back to EPUB++.
- Otherwise topos asks EPUB++ for each book's text (`extractText`, one string per spine item,
  paragraphs separated by `\n\n`, parsed on Obsidian's main thread one book at a time, which is
  slow for a library), searches the sections like notes, and asks for each hit's range CFI. On
  mobile this only happens with **Search new EPUBs on this device**; otherwise books wait for a
  computer to index them. A book EPUB++ can't read is kept with no references until it changes.

Offsets continue through the book, so position order is book order. topos registers a "Bible
references" provider: EPUB++ draws them (topos picks the style: dashed underline and glow), lists
them in a sidebar tab, and offers *Save as highlight* next to topos's menu items. Results in the
sidebar open with EPUB++'s `open(file, cfi)`.

Either way the CFIs match the reader's exactly.

## Design

- **One engine instance**: the WASM module is embedded in `main.js` and initialized once on load
  (no network, works on mobile).
- **Offsets**: everything uses UTF-16 offsets (`OffsetUnit.Utf16`), which is what CodeMirror
  and JavaScript strings use, so matches map straight to editor positions.
- **Pure logic is separate from Obsidian** (`src/core/`: filters, link templates, search
  results, sorting), so it is unit tested with Node; the Obsidian glue stays thin.
- **Editor decorations** only search the visible lines and update as you scroll or type.
- **Indexing never blocks the editor**: the vault is searched in a Web Worker with its own copy of
  the engine (the worker's code is bundled separately and embedded in `main.js`; the WebAssembly
  is sent to it at startup rather than embedded twice), in batches of up to 200 files or 4M
  characters. If workers are unavailable it falls back to the main thread, yielding between
  files. The sidebar re-renders at most twice a second while results arrive.
- **The index** (`topos-bible-index`, through the bindings' `ToposIndex`; `src/core/index.ts`
  wraps it, `src/indexers/vault.ts` keeps it on disk): each file's references in a compact binary
  form (about 15 bytes each), with its size, modification time, and for text files a content
  hash. At startup the packs are read and each file checked: unchanged files aren't searched, and
  files synced from another device (new times, same size) are matched by hash (notes) or size
  (EPUBs) and confirmed in this device's own packs. Each device writes only its own packs (32 per
  device, by path hash, written a few seconds after a change), so syncing never conflicts; other
  devices' packs are read again when Obsidian comes back to the front. EPUB details (CFIs and
  about 200 characters around each reference) are per book and read when a page or book needs
  them. The JSON cache from before (`epub-index.json`) is deleted.
- **Queries run in the engine**: the sidebar and *Go to a reference* send the filters as a
  `ToposQuery` (the CLI's rules), and the engine filters, sorts, and counts, keeping every
  reference's order until the index changes; only the page shown becomes objects. Notes' lines
  are read from the notes for the page shown.
- **EPUBs** (`src/epub/`) are searched by the CLI when it's the engine, else through EPUB++: the
  text comes from its API, searching uses the same worker, and `src/core/epub.ts` maps section
  hits to the book.
- **Optional CLI engine (desktop)**: runs `topos -m index --no-config [--cache] -- <files>` on the
  files that need searching, and streams each file's entry into the index. Its entries are
  identical to the built-in engine's (tested). Edits during a pass win over its older results.

## Library additions

The bindings gained what the filters need: `books()`, `genres()`, `find_book()`, and
`find_genre()` (on top of `search`, `parse`, `complete`, `contains`, `overlaps`, `verses`, and
`verse_ranges`). Until a release includes them, the plugin builds against the local package in
`crates/topos-ffi/dist/wasm/pkg`.

## Not in the first version

- PDF search inside the vault (needs the native formats crate)
- Book context for notes about one book (`--context-book`)
- Diagnostics for references that don't exist (needs `problems` in the bindings)

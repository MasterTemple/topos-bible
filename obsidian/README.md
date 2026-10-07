# Topos Bible for Obsidian

Find, search, filter, and autocomplete Bible references in your vault, and open them in
[Literal Word](https://app.literalword.com), BibleHub, lets.bible, BibleGateway, YouVersion, or any site with
a link template. Built on [topos-bible](../README.md) (Rust compiled
to WebAssembly, embedded in the plugin, so it works offline and on mobile). The design is in
[doc/obsidian-plugin.md](../doc/obsidian-plugin.md).

## Features

**Verse search sidebar** (the ribbon's book icon, or *Open verse search*), the CLI as a GUI:

- Search the whole vault, the current note, or a folder
- Open **Filters** to filter by testament (click OT/NT to include, again to exclude), genre,
  and book, each included or excluded, like `-t`, `-g`, `-b`, and `--exclude-*`. A testament
  narrows the rest (NT + Pauline Epistles is Paul's letters), included genres and books add up
  (Pentateuch + Revelation), and exclusions always win. The header shows how many filters are on
  and how many books they leave, and warns when they contradict each other: OT + Pauline
  Epistles, Books: Genesis with Inside: Romans 8, or Inside: John 3:16 with Not: John 3
- Filter by passage, each with as many passages as you like:
  - **Inside**: entirely within the passage (`-i`)
  - **Names**: names one of its verses; a whole chapter like `John 3` doesn't count for
    `John 3:16` (`-o`)
  - **Overlaps**: shares any verse, whole chapters included (`--any-overlap`)
  - **Exactly**: the same verses, however written (`--exact-overlap`)
  - **Not**: shares no verse (`--exclude-overlap`)
- Every filter input lists its options as soon as you click it (every book, then every chapter,
  then every verse, up to 176), narrowing as you type. In passage inputs, Enter adds a finished
  reference (`Add "John 3:16"` is the first choice) and Tab completes. Remove a chip with × or a
  middle-click
- Sort by order in your notes or order in the Bible; group by note or by book; show lines of
  context around each result (±1 to ±10 lines, read from the notes). All remembered
- **Saved searches**: name the current filters (and folder) with *Save*, pick one from the list,
  or open one with the *Open a saved search* command. Each is stored as the CLI's options, like
  `Sermons --nt -g "Pauline Epistles" -o "John 1"`, which you can also write or edit under
  *Settings → Saved searches*; the filter panel shows the current filters in that form. The same
  text works as a CLI named query (`~/.config/topos/queries.toml`, `topos -q NAME`). Saved searches
  from before 0.4.0, when `-o` meant any overlap, are rewritten once to keep their meaning
- Each result shows its line with the reference highlighted; click to jump there, or ↗ to open
  its link
- Results update as notes change

**In the editor**

- Autocomplete while typing: chapters after a book (`John ` → `John 1`…), verses after a colon,
  range ends after a dash, and book names (`1 Co` → `1 Corinthians`). Book names only complete
  for capitalized words by default, so ordinary prose isn't interrupted
- References glow faintly with a dashed underline in your accent color (or one you pick), and
  highlight on hover; Ctrl/Cmd-click or a middle click opens their links (a setting allows plain
  clicks, and another lets a tap open them on mobile when you aren't typing). With links turned off, they aren't marked
- Right-click a reference: open its link, find references to those verses, copy as OSIS
- In reading view, callouts, tables, and embedded notes, references are marked the same way, not
  as ordinary links: Ctrl/Cmd-click opens them on desktop, and a tap on mobile

**Commands**

| Command | |
|---|---|
| Open verse search | The sidebar |
| Search references in the current note | The sidebar, scoped to the note |
| Open a saved search | The sidebar with a saved search's filters |
| Go to a reference in the vault | Type a passage, pick a note that references it |
| Insert a verse reference | A dialog with autocomplete; books and chapters keep it open to refine |
| Find references to the verses under the cursor | The sidebar, filtered to overlapping references |
| Open the reference under the cursor in the browser | Its link (see [Links](#links)) |
| Copy the reference under the cursor as OSIS | `John.3.16-John.3.18` |
| Normalize references in the selection or note | `jn 3:16` → `John 3:16`, in the chosen style |

**Settings**: reference style (`John 3:16`, `Jn 3:16`, or `John.3.16`), where references open
(see [Links](#links)), the references' color, editor and reading-view links, whether editor clicks
need Ctrl/Cmd, whether a tap opens references in the editor on mobile (when the keyboard isn't open), autocomplete and book-name completion, number of suggestions, the reference format
(the CLI's `--psg-fmt` fields: separators, joining adjacent verses, the chapter in single-chapter
books, with a preview; used by completions, the sidebar, the dialogs, and normalizing), file
extensions to
search, whether to search EPUBs (below), excluded folders, and the search engine.

### EPUBs

With **Search EPUB files** on (off by default) and the [EPUB++](https://github.com/MasterTemple/epub-plus-plus)
plugin enabled, EPUBs in the vault are searched too. EPUB++ opens EPUBs in Obsidian; it extracts
each book's text for topos (paragraphs separated by a blank line, so a reference never runs from
one into the next) and turns the references' positions into CFIs. Then:

- The sidebar lists EPUB results with their chapter, and clicking one opens the book there.
- In the book, references get the same dashed underline and glow as in notes (in the reference
  color). Hovering shows the reference; Ctrl/Cmd-click (or middle-click) opens its link.
- EPUB++'s **Bible references** sidebar tab lists the book's references by chapter, with a filter.
- Right-click a reference (tap or hold it on mobile) to open it, find references to the same
  verses, copy it as OSIS, or **Save as highlight** / **Save with comment**, which turn it into an
  EPUB++ highlight in the book's annotation file.

Books are searched once, on any device (see below). With the CLI engine, the `topos` command
searches them, which is much faster for a library (EPUB++ extracts one book at a time on
Obsidian's thread); books it can't read still go through EPUB++. A phone or tablet doesn't search
books at all unless **Search new EPUBs on this device** is on: it uses what a computer indexed.

### The reference index

Every file's references are kept in an index in the plugin's folder (`index/`), which syncs with
the rest of your settings. So each file is searched once, on whichever device sees it first, and
Obsidian starts without searching anything that hasn't changed. Another device (like your phone)
uses those results too: files synced to it have new modification times, so a note is matched by
its contents and a book by its size. Each device writes its own files (`index/packs/<device>-*.bin`),
so syncing never conflicts; EPUBs' CFIs and the text around each reference are kept per book
(`index/details/`) and read only when shown.

The index is compact (about 15 bytes a reference; a million references take 16 MB), and the
sidebar's filtering, sorting, and counting run in the engine, a page at a time. **Rebuild** in the
settings searches every file again.

### Large vaults

New and changed files are searched in a background thread (a Web Worker), so Obsidian stays
responsive; results appear in the sidebar as batches finish. On desktop, **Search engine → topos
CLI** runs the native [`topos`](../crates/topos-cli) command on them instead, which is much faster
for many files and EPUBs and can cache results between runs (`--cache`):

```sh
cargo install topos-bible-cli
```

The plugin looks for `~/.cargo/bin/topos`, then `topos` on your PATH; set the path in the settings
otherwise (the **Test** button checks it). It runs `topos -m index` on the files that need
searching, named one by one (so a `.gitignore` can't skip them). It needs a `topos` new enough to
have `-m index`; with an older one, or if it fails, the plugin says so and uses the built-in
engine. Notes you edit are always searched by the built-in engine.

## Links

References open in Literal Word by default. The settings offer BibleHub, lets.bible,
BibleGateway, and YouVersion, no links at all, or a link template of your own, with a preview as you type:

| Site | Template |
|---|---|
| Literal Word | `https://app.literalword.com/{book.id}/{chapter}[/{verse}]` |
| BibleHub | `https://biblehub.com/{book.biblehub}/{chapter}[-{verse}].htm` |
| lets.bible | `https://lets.bible/bible/{book\|lower\|kebab}/{chapter}[?v={verse}]` |
| BibleGateway | `https://www.biblegateway.com/passage/?search={reference}` |
| YouVersion | `https://www.bible.com/bible/59/{book.usfm}.{chapter}[.{verse}][-{end_verse}]` |

Edit one to choose a translation: `https://app.literalword.com/esv/...`, `...?search={reference}&version=NIV`,
or YouVersion's number for it (59 is the ESV, 111 the NIV, 1 the KJV).

Placeholders, for 1 Corinthians 13:4-7 (values are URL-encoded):

| Placeholder | Value |
|---|---|
| `{book}`, `{book.name}` | `1 Corinthians` |
| `{book.abbreviation}` | `1 Cor` |
| `{book.osis}` | `1Cor` |
| `{book.id}` | `46` (1-66) |
| `{book.usfm}` | `1CO` (YouVersion's codes) |
| `{book.biblehub}` | `1_corinthians` (BibleHub's names: `psalms`, `songs`) |
| `{chapter}`, `{verse}` | `13`, `4`: where the reference starts (no verse for a whole chapter) |
| `{end_verse}` | `7`: the end of the first range, when it is in one chapter |
| `{end_chapter}` | The last chapter of the first part, when it spans chapters (`Gen 1-2` → `2`) |
| `{reference}` | `1 Corinthians 13:4-7`, the whole reference |
| `{osis}` | `1Cor.13.4-1Cor.13.7`, the whole reference |

Filters change a value before it goes in: `{book|lower|kebab}` is `1-corinthians`. They are
`lower`, `upper`, `snake` (spaces to `_`), `kebab` (spaces to `-`), and `compact` (no spaces).

A part in `[ ]` is left out when a placeholder in it has no value, so `{chapter}[/{verse}]` is
`3/16` for John 3:16 and `3` for John 3. A placeholder with no value outside `[ ]` means no link
(`{book.usfm}` for a book outside the 66, from custom data). Most sites open one verse or
chapter, so a reference like `John 3:16; 4:1` opens at its first verse; `{reference}` and
`{osis}` carry all of it. Changing the site updates open notes, reading view, and the sidebar
right away.

## Build and install

The plugin uses the `topos-bible` package built from this repository, since it needs the newest
bindings (books, genres, and lookups by name):

```sh
# Once: cargo install boltffi_cli --version 0.31.0, rustup target add wasm32-unknown-unknown
(cd ../crates/topos-ffi && npm install && npm run pack:wasm)

npm install
scripts/install.sh ~/path/to/vault   # builds, then copies main.js, manifest.json, styles.css
```

Then enable **Topos Bible** under *Settings → Community plugins*. Once a `topos-bible` release
includes these bindings, the dependency can point at the npm version instead.

## Development

- `npm run dev` rebuilds `main.js` on every change
- `npm test` runs the tests with Node: the core logic, the bundled WebAssembly loading, the
  bundled worker (in a Node worker thread), the sidebar in a DOM (happy-dom: inputs, chips,
  context lines), the built plugin against a stand-in for Obsidian's
  API (loading, indexing with both engines, commands, and autocomplete), and the sidebar rendered
  to HTML. The CLI tests use `../target/debug/topos` (`cargo build -p topos-bible-cli`) and are skipped
  without it
- `npm run typecheck`

`src/indexers/` has the Web Worker (`worker.ts`, bundled separately and embedded in `main.js` by
`esbuild.config.mjs`), the CLI runner, and `vault.ts`, which keeps the index on disk and up to
date. `src/core/` has no Obsidian imports (filters, the index's wrapper, sorting, completions, link
templates, settings data), so it is tested directly; the rest connects it to Obsidian.

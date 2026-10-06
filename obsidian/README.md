# Topos Bible for Obsidian

Find, search, filter, and autocomplete Bible references in your vault, and open them in
[Literal Word](https://app.literalword.com). Built on [topos-bible](../README.md) (Rust compiled
to WebAssembly, embedded in the plugin, so it works offline and on mobile). The design is in
[doc/obsidian-plugin.md](../doc/obsidian-plugin.md).

## Features

**Verse search sidebar** (the ribbon's book icon, or *Open verse search*), the CLI as a GUI:

- Search the whole vault, the current note, or a folder
- Filter by testament (click OT/NT to include, again to exclude), genre, and book, each
  included or excluded, like `-t`, `-g`, `-b`, and `--exclude-*`
- Filter by passage: **Inside** (entirely within, like `-i`), **Overlapping** (shares a verse,
  like `-o`), and **Outside** (`--outside`); the inputs autocomplete references
- Sort by order in your notes or order in the Bible; group by note or by book
- Each result shows its line with the reference highlighted; click to jump there, or ↗ to open
  it in Literal Word
- Results update as notes change

**In the editor**

- Autocomplete while typing: chapters after a book (`John ` → `John 1`…), verses after a colon,
  range ends after a dash, and book names (`1 Co` → `1 Corinthians`). Book names only complete
  for capitalized words by default, so ordinary prose isn't interrupted
- References are underlined; Ctrl/Cmd-click opens them in Literal Word (a setting allows plain
  clicks)
- Right-click a reference: open in Literal Word, find references to those verses, copy as OSIS
- In reading view, references are links to Literal Word

**Commands**

| Command | |
|---|---|
| Open verse search | The sidebar |
| Search references in the current note | The sidebar, scoped to the note |
| Go to a reference in the vault | Type a passage, pick a note that references it |
| Insert a verse reference | A dialog with autocomplete; books and chapters keep it open to refine |
| Find references to the verses under the cursor | The sidebar, filtered to overlapping references |
| Open the reference under the cursor in Literal Word | |
| Copy the reference under the cursor as OSIS | `John.3.16-John.3.18` |
| Normalize references in the selection or note | `jn 3:16` → `John 3:16`, in the chosen style |

**Settings**: reference style (`John 3:16`, `Jn 3:16`, or `John.3.16`), Literal Word translation
(NASB, LSB, ESV, NKJV, KJV, or its default), editor and reading-view links, whether editor clicks
need Ctrl/Cmd, autocomplete and book-name completion, number of suggestions, file extensions to
search, and excluded folders.

Literal Word links open one verse or chapter (it doesn't support ranges), so a reference opens
at its first verse.

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
  built plugin against a stand-in for Obsidian's API (loading, indexing, commands, and
  autocomplete), and the sidebar rendered to HTML
- `npm run typecheck`

`src/core/` has no Obsidian imports (filters, search index, sorting, completions, Literal Word
links, settings data), so it is tested directly; the rest connects it to Obsidian.

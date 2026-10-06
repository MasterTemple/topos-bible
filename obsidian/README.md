# Topos Bible for Obsidian

Find, search, filter, and autocomplete Bible references in your vault, and open them in
[Literal Word](https://app.literalword.com). Built on [topos-bible](../README.md) (Rust compiled
to WebAssembly, embedded in the plugin, so it works offline and on mobile). The design is in
[doc/obsidian-plugin.md](../doc/obsidian-plugin.md).

## Features

**Verse search sidebar** (the ribbon's book icon, or *Open verse search*), the CLI as a GUI:

- Search the whole vault, the current note, or a folder
- Open **Filters** to filter by testament (click OT/NT to include, again to exclude), genre,
  and book, each included or excluded, like `-t`, `-g`, `-b`, and `--exclude-*`. A testament
  narrows the rest (NT + Pauline Epistles is Paul's letters), included genres and books add up
  (Pentateuch + Revelation), and exclusions always win. The header shows how many filters are on
  and how many books they leave, and warns when they contradict each other (OT + Pauline
  Epistles)
- Filter by passage: **Inside** (entirely within, like `-i`), **Overlapping** (shares a verse,
  like `-o`), and **Outside** (`--outside`), each with as many passages as you like
- Every filter input lists its options as soon as you click it (every book, then every chapter,
  then every verse, up to 176), narrowing as you type. In passage inputs, Enter adds a finished
  reference (`Add "John 3:16"` is the first choice) and Tab completes. Remove a chip with × or a
  middle-click
- Sort by order in your notes or order in the Bible; group by note or by book (remembered)
- **Saved searches**: name the current filters (and folder) with *Save*, pick one from the list,
  or open one with the *Open a saved search* command. Each is stored as the CLI's options, like
  `Sermons --nt -g "Pauline Epistles" -o "John 1"`, which you can also write or edit under
  *Settings → Saved searches*; the filter panel shows the current filters in that form. The same
  text works as a CLI named query (`~/.config/topos/queries.toml`, `topos -q NAME`)
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
| Open a saved search | The sidebar with a saved search's filters |
| Go to a reference in the vault | Type a passage, pick a note that references it |
| Insert a verse reference | A dialog with autocomplete; books and chapters keep it open to refine |
| Find references to the verses under the cursor | The sidebar, filtered to overlapping references |
| Open the reference under the cursor in Literal Word | |
| Copy the reference under the cursor as OSIS | `John.3.16-John.3.18` |
| Normalize references in the selection or note | `jn 3:16` → `John 3:16`, in the chosen style |

**Settings**: reference style (`John 3:16`, `Jn 3:16`, or `John.3.16`), Literal Word translation
(NASB, LSB, ESV, NKJV, KJV, or its default), editor and reading-view links, whether editor clicks
need Ctrl/Cmd, autocomplete and book-name completion, number of suggestions, file extensions to
search, excluded folders, and the search engine (below).

### Large vaults

The vault is indexed in a background thread (a Web Worker), so Obsidian stays responsive while it
runs; results appear in the sidebar as batches finish. On desktop, **Search engine → topos CLI**
runs the native [`topos`](../crates/topos-cli) command over the vault folder instead, which is
faster for very large vaults and can cache results between runs (`--cache`):

```sh
cargo install --git https://github.com/MasterTemple/topos-bible topos-cli
```

The plugin looks for `~/.cargo/bin/topos`, then `topos` on your PATH; set the path in the settings
otherwise (the **Test** button checks it). It runs `topos --ext` with the plugin's file
extensions, so EPUBs, PDFs, and other files in the vault aren't searched. It needs a `topos` new
enough to have `--ext` and to report UTF-16 positions in its JSON output; with an older one, or if it fails, the plugin says so and uses the
built-in engine. Notes you edit are always indexed by the built-in engine. Like ripgrep, the CLI
skips files ignored by a `.gitignore`.

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
  bundled worker (in a Node worker thread), the built plugin against a stand-in for Obsidian's
  API (loading, indexing with both engines, commands, and autocomplete), and the sidebar rendered
  to HTML. The CLI tests use `../target/debug/topos` (`cargo build -p topos-cli`) and are skipped
  without it
- `npm run typecheck`

`src/indexers/` has the Web Worker (`worker.ts`, bundled separately and embedded in `main.js` by
`esbuild.config.mjs`) and the CLI runner. `src/core/` has no Obsidian imports (filters, search index, sorting, completions, Literal Word
links, settings data), so it is tested directly; the rest connects it to Obsidian.

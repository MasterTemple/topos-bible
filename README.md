# τόπος Bible

> [τόπος](https://biblehub.com/greek/5117.htm): spot, location, position

Locate Bible verses in text, files, and entire directories!

## Install

```sh
# The `topos` command-line tool
cargo install topos-bible-cli

# The library, from Rust, JavaScript/TypeScript, or Python
cargo add topos-bible
npm install topos-bible
pip install topos-bible
```

## Neovim

**topos-bible.nvim** (this repository) runs the language server and searches your notes, with
Telescope pickers. With [lazy.nvim](https://lazy.folke.io):

```lua
{
  'MasterTemple/topos-bible',
  name = 'topos-bible.nvim',
  main = 'topos',
  -- Builds topos-lsp and the topos CLI from this checkout (needs Rust: https://rustup.rs)
  build = 'cargo build --release -p topos-lsp -p topos-bible-cli',
  dependencies = { 'nvim-telescope/telescope.nvim' }, -- optional
  ft = { 'markdown', 'text' },
  cmd = { 'ToposSearch', 'ToposQuery', 'ToposExplicitOverlap', 'ToposAnyOverlap',
          'ToposExactOverlap', 'ToposInside', 'ToposExcludeOverlap', 'ToposBuild', 'ToposLspStart' },
  opts = {
    -- topos-lsp's settings: the CLI's config.toml keys, which these override
    settings = {
      format = 'name',                        -- uses full name, use 'abbreviation' for Jn 3:16
      ['psg-fmt'] = { join_adjacent = true }, -- 3:16-18, not 3:16,17,18
      ['reference-diagnostics'] = 'hint',     -- 'info' (default), 'hint', or 'never'
      ['inlay-hints'] = 'never',              -- 'changed' (default), 'always', 'osis', 'never'
      hover = { 'osis', 'book', 'genres' },   -- also name, abbreviation, testament, verses, location, written, bcv
      ext = 'md,txt',                         -- what references and searches cover
    },
  },
}
```

- **Language server**: completion, hover, inlay hints, symbols, and diagnostics (each
  reference as formatted, and warnings for verses that don't exist); go to
  references (`grr`) lists the references that are exactly the one under the cursor, and the
  code actions (`gra`) search for it by explicit overlap, any overlap, exact overlap, or inside it
- **Commands**: `:ToposSearch` (every reference), `:ToposQuery -g Wisdom` (the CLI's options,
  completed with Tab), and `:ToposExplicitOverlap John 3:16`, `:ToposAnyOverlap`,
  `:ToposExactOverlap`, `:ToposInside`, `:ToposExcludeOverlap` (the reference under the cursor
  by default)
- **Telescope**: `:Telescope topos`, plus a picker per search; without Telescope, results open
  in the quickfix list

The plugin and the CLI share `~/.config/topos/config.toml`. See `:help topos-bible`, and
`:checkhealth topos` if something doesn't work. `tests/nvim/run.sh` tests it in headless Neovim.

<details>
<summary>Screenshots</summary>

![Neovim Quick Switcher Integration](./doc/imgs/neovim-quick-switcher.png)

![Neovim Telescope Integration](./doc/imgs/neovim-telescope.png)

</details>

## Crates

| Crate (folder) | What it is |
|---|---|
| [`topos-bible`](./crates/topos-lib/README.md) (`topos-lib`) | The core: book data, the segment grammar, resolving, searching, formatting, OSIS, and autocomplete |
| [`topos-bible-formats`](./crates/topos-formats/README.md) (`topos-formats`) | Locations in HTML, SRT/WebVTT/SBV, EPUB, JSON, XML, and PDF (`pdf` feature) |
| [`topos-bible-cli`](./crates/topos-cli/README.md) (`topos-cli`) | `topos`, a ripgrep-style search tool |
| [`topos-lsp`](./crates/topos-lsp/README.md) | A language server: completion, hover, and document symbols |
| [`topos-ffi`](./crates/topos-ffi) | Bindings for TypeScript/WASM, Python, Swift, and Kotlin via [BoltFFI](https://boltffi.dev) |
| [`obsidian`](./obsidian/README.md) | An Obsidian plugin: verse search with filters, autocomplete, and links to Bible sites |

See [ROADMAP.md](./ROADMAP.md) for what is planned and [doc/architecture.md](./doc/architecture.md) for how it fits together.

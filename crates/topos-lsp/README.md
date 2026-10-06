# τόπος language server

`topos-lsp` speaks the Language Server Protocol over stdin and stdout:

- **Completion**: book names (`1 Co` → `1 Corinthians`), then chapters and verses from the
  book's versification; each completion rewrites the whole reference (`jn 3:1` → `John 3:16`)
- **Hover**: the reference under the cursor, normalized, with its OSIS id
- **Inlay hints**: after each reference, how it's written in your format (where that differs,
  like `jn 3:16` → `John 3:16`), or its OSIS id
- **Document symbols**: every reference in the file, for outlines and pickers
- **Go to references**: every reference in the workspace that is exactly the one under the
  cursor (`--exact-overlap`), however it's written
- **Code actions**: on a reference, search the workspace for it: `Search "John 3:16" for
  explicit overlap`, `... for any overlap`, `... for exact overlap`, and `Search inside "John 3"`
- **Diagnostics**: an information diagnostic on each reference with how it's written in your
  format and its OSIS id (`John 3:16 (John.3.16)`, code `reference`), and warnings for references
  that do not exist, like `John 3:99` (code `missing`)

Workspace searches cover every text file in the workspace folders, skipping hidden, binary, and
ignored files (`.gitignore`, `.ignore`, `.toposignore`), like the CLI. Open files are searched
as they are in the editor, including unsaved changes.

Install with:

```sh
cargo install --git https://github.com/MasterTemple/topos-bible topos-lsp
```

## Settings

The server reads the CLI's `~/.config/topos/config.toml` (or `$XDG_CONFIG_HOME/topos/config.toml`),
so one file configures both. It uses these keys and ignores the others:

| Key | Effect |
|---|---|
| `format`, `psg-fmt`, `fmt-*` | How references are written in completions, hover, hints, symbols, and action titles (see the CLI's "Writing references") |
| `data`, `merge-data`, `remove-data` | Custom book data |
| `context-book`, `context-heading` | References without a book (`3:16`) |
| `ext` | Extensions searched in the workspace, like `"md,txt"` (all text files by default) |
| `inlay-hints` | `"changed"` (default), `"always"`, `"osis"`, or `"never"` |
| `reference-diagnostics` | The diagnostic on each reference: `"info"` (default), `"hint"`, or `"never"` |

The editor's settings override the file: as initialization options, or as `settings` (changed
while the server runs, without a restart). They take the same keys, at the top level or under
`topos`. Two more choose the file: `config` (another path) and `no-config` (none). A `format`
can also be a whole object, like `psg-fmt`. Settings that can't be used are reported as a
warning, and the previous ones stay.

## Neovim

The [topos-bible.nvim](../../README.md#neovim) plugin sets up the server, builds it, opens the
code actions' results in Telescope (or the quickfix list), and adds search commands and pickers.
See `:help topos-bible`.

Without the plugin (Neovim 0.11+):

```lua
vim.lsp.config('topos', {
  cmd = { 'topos-lsp' },
  filetypes = { 'markdown', 'text' },
  root_markers = { '.git', '.obsidian' },
  init_options = { ['psg-fmt'] = { join_adjacent = true }, ['inlay-hints'] = 'always' },
})
vim.lsp.enable('topos')
```

Go to references and completion then work. The code actions need a `vim.lsp.commands['topos.search']`
handler to show their results (see below; the plugin's is in `lua/topos/init.lua`).

## Code actions in other editors

LSP has no standard way for a server to open the editor's references list. The code actions
run the `topos.search` command, which returns the matching locations
(`workspace/executeCommand` with `{ "mode": "explicit-overlap" | "any-overlap" |
"exact-overlap" | "inside", "passage": "John 3:16" }`); the editor shows them.

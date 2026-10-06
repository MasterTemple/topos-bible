# τόπος language server

`topos-lsp` speaks the Language Server Protocol over stdin and stdout:

- **Completion**: book names (`1 Co` → `1 Corinthians`), then chapters and verses from the
  book's versification; each completion rewrites the whole reference (`jn 3:1` → `John 3:16`)
- **Hover**: the reference under the cursor, normalized, with its OSIS id
- **Document symbols**: every reference in the file, for outlines and pickers
- **Diagnostics**: warnings for references that do not exist, like `John 3:99`

Install with:

```sh
cargo install --git https://github.com/MasterTemple/topos-bible topos-lsp
```

## Neovim (0.11+)

```lua
vim.lsp.config('topos', {
  cmd = { 'topos-lsp' },
  filetypes = { 'markdown', 'text' },
})
vim.lsp.enable('topos')
```

References then show up in `vim.lsp.buf.document_symbol()` (or Telescope's
`lsp_document_symbols`), and completion works with any LSP completion plugin.

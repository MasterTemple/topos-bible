-- The plugin's options and their defaults (see :help topos-bible-setup)
local M = {}

M.defaults = {
  -- topos-lsp's settings, overriding ~/.config/topos/config.toml (the CLI's keys), like
  -- { format = 'abbreviation', ['psg-fmt'] = { join_adjacent = true },
  --   ['inlay-hints'] = 'never', ['reference-diagnostics'] = 'hint', ext = 'md,txt' }
  settings = {},
  lsp = {
    enabled = true,
    filetypes = { 'markdown', 'text' },
    -- The folder searched by go-to-references, the code actions, and the search commands
    root_markers = { '.git', '.obsidian' },
    -- Show how each reference is written in your format (see `settings['inlay-hints']`)
    inlay_hints = true,
    -- The same as the top-level `settings` (which win where both set a key)
    settings = {},
  },
  -- Paths to the binaries; nil uses the plugin's own build, then your PATH
  cmd = { lsp = nil, cli = nil },
  -- Build the binaries when setup can't find them (takes a few minutes the first time)
  auto_build = true,
  -- 'telescope', 'quickfix', or 'auto' (Telescope if it is installed)
  picker = 'auto',
}

M.options = vim.deepcopy(M.defaults)

function M.set(opts)
  M.options = vim.tbl_deep_extend('force', vim.deepcopy(M.defaults), opts or {})
end

-- The language server's settings: `lsp.settings`, then the top-level `settings`
function M.lsp_settings()
  return vim.tbl_deep_extend('force', M.options.lsp.settings or {}, M.options.settings or {})
end

return M

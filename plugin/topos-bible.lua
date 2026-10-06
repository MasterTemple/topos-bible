-- The commands exist from startup (so lazy.nvim can load the plugin on first use); setup() is
-- what starts the language server
if vim.g.loaded_topos_bible then
  return
end
vim.g.loaded_topos_bible = true
require('topos.commands').create()

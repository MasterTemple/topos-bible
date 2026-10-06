-- :checkhealth topos
local M = {}

function M.check()
  local bin = require('topos.bin')
  local config = require('topos.config')
  vim.health.start('topos-bible.nvim')
  for _, kind in ipairs({ 'lsp', 'cli' }) do
    local name = kind == 'lsp' and 'topos-lsp' or 'topos'
    local path = bin.path(kind)
    if path then
      local version = vim.system({ path, '--version' }, { text = true }):wait().stdout or ''
      vim.health.ok(('%s: %s %s'):format(name, path, vim.trim(version)))
    else
      vim.health.error(name .. ' not found', { 'Run :ToposBuild (needs Rust: https://rustup.rs)' })
    end
  end
  if vim.fn.executable('cargo') == 1 then
    vim.health.ok('cargo is available to build with')
  else
    vim.health.warn('cargo not found: :ToposBuild needs Rust (https://rustup.rs)')
  end
  if pcall(require, 'telescope') then
    vim.health.ok('Telescope: results open in its pickers')
  else
    vim.health.info('Telescope is not installed: results open in the quickfix list')
  end
  if #vim.lsp.get_clients({ name = 'topos' }) > 0 then
    vim.health.ok('topos-lsp is running')
  elseif config.options.lsp.enabled then
    vim.health.info('topos-lsp is not attached to any buffer yet (open a ' .. table.concat(config.options.lsp.filetypes, ' or ') .. ' file)')
  end
end

return M

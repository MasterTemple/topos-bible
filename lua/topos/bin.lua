-- Finding the topos binaries: a configured path, the plugin's own build, then your PATH
local config = require('topos.config')

local M = {}

local exe = vim.fn.has('win32') == 1 and '.exe' or ''

-- The repository this plugin was installed from (it has the Rust sources to build)
function M.root()
  local source = debug.getinfo(1, 'S').source:sub(2)
  return vim.fn.fnamemodify(source, ':p:h:h:h')
end

-- The plugin's build of `name`, like target/release/topos-lsp
function M.built(name)
  return vim.fs.joinpath(M.root(), 'target', 'release', name .. exe)
end

-- The binary to run for 'lsp' (topos-lsp) or 'cli' (topos), or nil if there is none
function M.path(kind)
  local configured = config.options.cmd[kind]
  if configured then
    return vim.fn.executable(configured) == 1 and configured or nil
  end
  local name = kind == 'lsp' and 'topos-lsp' or 'topos'
  local built = M.built(name)
  if vim.fn.executable(built) == 1 then
    return built
  end
  local found = vim.fn.exepath(name)
  return found ~= '' and found or nil
end

return M

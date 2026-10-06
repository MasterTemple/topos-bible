-- Building topos-lsp and topos (the CLI) from the plugin's own checkout with Cargo
local bin = require('topos.bin')

local M = {}

M.command = { 'cargo', 'build', '--release', '-p', 'topos-lsp', '-p', 'topos-bible-cli' }

-- Builds unless both binaries are already there (`force` builds anyway); `done(ok)` runs after
function M.build(force, done)
  done = done or function() end
  if not force and bin.path('lsp') and bin.path('cli') then
    vim.notify('topos: already built (use :ToposBuild! to rebuild)')
    return done(true)
  end
  if vim.fn.executable('cargo') == 0 then
    vim.notify('topos: building needs Rust (https://rustup.rs)', vim.log.levels.ERROR)
    return done(false)
  end
  vim.notify('topos: building topos-lsp and topos (this takes a few minutes the first time)...')
  vim.system(M.command, { cwd = bin.root(), text = true }, function(result)
    vim.schedule(function()
      if result.code == 0 then
        vim.notify('topos: built topos-lsp and topos')
        done(true)
      else
        local lines = vim.split(result.stderr or '', '\n', { trimempty = true })
        local tail = table.concat(vim.list_slice(lines, math.max(1, #lines - 15)), '\n')
        vim.notify('topos: the build failed:\n' .. tail, vim.log.levels.ERROR)
        done(false)
      end
    end)
  end)
end

return M

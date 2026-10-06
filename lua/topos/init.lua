-- topos-bible.nvim: find, search, and complete Bible references (see :help topos-bible)
local bin = require('topos.bin')
local config = require('topos.config')

local M = {}

local started = false

-- Shows the locations from the code actions' `topos.search` like go-to-references does
local function search_command(command, ctx)
  local client = assert(vim.lsp.get_client_by_id(ctx.client_id))
  client:request('workspace/executeCommand', command, function(err, locations)
    if err then
      return vim.notify('topos: ' .. err.message, vim.log.levels.ERROR)
    end
    local items = vim.lsp.util.locations_to_items(locations or {}, client.offset_encoding)
    require('topos.search').show(command.title, items)
  end, ctx.bufnr)
end

-- Configures and enables topos-lsp (once its binary exists)
function M.start_lsp()
  local lsp = config.options.lsp
  local path = bin.path('lsp')
  if started or not lsp.enabled or not path then
    return
  end
  started = true
  vim.lsp.config('topos', {
    cmd = { path },
    filetypes = lsp.filetypes,
    root_markers = lsp.root_markers,
    init_options = lsp.settings,
    settings = { topos = lsp.settings },
  })
  vim.lsp.enable('topos')
  if lsp.inlay_hints then
    vim.api.nvim_create_autocmd('LspAttach', {
      group = vim.api.nvim_create_augroup('topos_inlay_hints', { clear = true }),
      callback = function(args)
        local client = vim.lsp.get_client_by_id(args.data.client_id)
        if client and client.name == 'topos' then
          vim.lsp.inlay_hint.enable(true, { bufnr = args.buf })
        end
      end,
    })
  end
end

function M.setup(opts)
  config.set(opts)
  require('topos.commands').create()
  vim.lsp.commands['topos.search'] = search_command
  local ok, telescope = pcall(require, 'telescope')
  if ok then
    pcall(telescope.load_extension, 'topos')
  end
  if not config.options.lsp.enabled then
    return
  end
  if bin.path('lsp') and bin.path('cli') then
    M.start_lsp()
  elseif config.options.auto_build then
    require('topos.build').build(false, function(built)
      if built then
        M.start_lsp()
      end
    end)
  else
    vim.notify('topos: topos-lsp is not built; run :ToposBuild', vim.log.levels.WARN)
  end
end

return M

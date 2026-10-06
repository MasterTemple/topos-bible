-- topos-lsp for Neovim 0.11+: copy this into your config, or `dofile` it.
--
-- Go to references (`grr`, or Telescope's `lsp_references`) lists the references that are exactly
-- the one under the cursor. The code actions (`gra`) search for the others: explicit overlap, any
-- overlap, exact overlap, or inside it. Their results open the way references do: in Telescope's
-- picker if Telescope is installed, otherwise in the quickfix list.

vim.lsp.config('topos', {
  cmd = { 'topos-lsp' },
  filetypes = { 'markdown', 'text' },
  -- The folder searched by go-to-references and the code actions
  root_markers = { '.git', '.obsidian' },
  -- Optional: these override ~/.config/topos/config.toml (same keys as the CLI)
  -- init_options = {
  --   ['psg-fmt'] = { join_adjacent = true },
  --   format = 'abbreviation',
  --   ['inlay-hints'] = 'changed', -- or 'always', 'osis', 'never'
  -- },
})
vim.lsp.enable('topos')
vim.lsp.inlay_hint.enable(true)

-- Shows locations like go-to-references does: Telescope's picker, or the quickfix list
local function show_locations(title, locations, client)
  local items = vim.lsp.util.locations_to_items(locations, client.offset_encoding)
  local ok, pickers = pcall(require, 'telescope.pickers')
  if ok then
    local conf = require('telescope.config').values
    pickers
      .new({}, {
        prompt_title = title,
        finder = require('telescope.finders').new_table({
          results = items,
          entry_maker = require('telescope.make_entry').gen_from_quickfix({}),
        }),
        previewer = conf.qflist_previewer({}),
        sorter = conf.generic_sorter({}),
      })
      :find()
  else
    vim.fn.setqflist({}, ' ', { title = title, items = items })
    vim.cmd('botright copen')
  end
end

-- The code actions run `topos.search` on the server, which returns the locations it finds
vim.lsp.commands['topos.search'] = function(command, ctx)
  local client = assert(vim.lsp.get_client_by_id(ctx.client_id))
  client:request('workspace/executeCommand', command, function(err, locations)
    if err then
      vim.notify(err.message, vim.log.levels.ERROR)
    elseif not locations or #locations == 0 then
      vim.notify(command.title .. ': nothing found')
    else
      show_locations(command.title, locations, client)
    end
  end, ctx.bufnr)
end

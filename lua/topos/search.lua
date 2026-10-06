-- Searching with the topos CLI (`topos -m json`), and showing results in Telescope or quickfix
local bin = require('topos.bin')
local config = require('topos.config')

local M = {}

-- The search kinds for a single passage: the CLI flag and how a title describes it
M.kinds = {
  explicit_overlap = { flag = '-o', title = 'Search "%s" for explicit overlap' },
  any_overlap = { flag = '--any-overlap', title = 'Search "%s" for any overlap' },
  exact_overlap = { flag = '--exact-overlap', title = 'Search "%s" for exact overlap' },
  inside = { flag = '-i', title = 'Search inside "%s"' },
  exclude_overlap = { flag = '--exclude-overlap', title = 'Search outside "%s" (no overlap)' },
}

-- The folder to search: the language server's for this buffer, a root marker, or the cwd
function M.root(bufnr)
  bufnr = bufnr or 0
  for _, client in ipairs(vim.lsp.get_clients({ bufnr = bufnr, name = 'topos' })) do
    if client.root_dir then
      return client.root_dir
    end
  end
  return vim.fs.root(bufnr, config.options.lsp.root_markers) or vim.fn.getcwd()
end

-- Splits like a shell: `-g "Pauline Epistles" -b John` is 4 words
function M.split(text)
  local words, word, quote, started = {}, {}, nil, false
  for char in text:gmatch('.') do
    if quote then
      if char == quote then
        quote = nil
      else
        table.insert(word, char)
      end
    elseif char == '"' or char == "'" then
      quote, started = char, true
    elseif char:match('%s') then
      if started then
        table.insert(words, table.concat(word))
        word, started = {}, false
      end
    else
      table.insert(word, char)
      started = true
    end
  end
  if started then
    table.insert(words, table.concat(word))
  end
  return words
end

-- The CLI, or nil after saying how to get it
local function cli()
  local path = bin.path('cli')
  if not path then
    vim.notify('topos: the topos CLI is not built yet; run :ToposBuild', vim.log.levels.ERROR)
  end
  return path
end

-- One CLI match as a list item (filename, lnum, col, text) plus what the pickers show
local function item(line, root)
  local ok, m = pcall(vim.json.decode, line)
  if not ok or type(m) ~= 'table' or type(m.path) ~= 'string' then
    return nil
  end
  local path = m.path:gsub('^%./', '')
  local filename = path:match('^/') and path or vim.fs.joinpath(root, path)
  local line_text = vim.trim(m.line_text or '')
  return {
    filename = filename,
    path = path,
    lnum = m.line,
    col = m.column,
    end_lnum = m.end_line,
    end_col = m.end_column,
    reference = m.reference,
    text = m.reference .. ': ' .. line_text,
    line_text = line_text,
  }
end

-- Shows items the way go-to-references does: a Telescope picker, or the quickfix list
function M.show(title, items)
  if #items == 0 then
    return vim.notify(title .. ': nothing found')
  end
  local picker = config.options.picker
  local has_telescope = picker ~= 'quickfix' and pcall(require, 'telescope')
  if picker == 'telescope' and not has_telescope then
    vim.notify('topos: Telescope is not installed; using the quickfix list', vim.log.levels.WARN)
  end
  if not has_telescope then
    vim.fn.setqflist({}, ' ', { title = title, items = items })
    return vim.cmd('botright copen')
  end
  local conf = require('telescope.config').values
  require('telescope.pickers')
    .new({}, {
      prompt_title = title,
      finder = require('telescope.finders').new_table({
        results = items,
        entry_maker = function(entry)
          local where = string.format('%s:%d', entry.path or vim.fn.fnamemodify(entry.filename, ':.'), entry.lnum)
          local label = entry.reference and (entry.reference .. '  ') or ''
          local text = entry.line_text or entry.text or ''
          return {
            value = entry,
            display = label .. where .. '  ' .. text,
            ordinal = label .. where .. ' ' .. text,
            filename = entry.filename,
            lnum = entry.lnum,
            col = entry.col,
          }
        end,
      }),
      previewer = conf.qflist_previewer({}),
      sorter = conf.generic_sorter({}),
    })
    :find()
end

-- Whether any argument is an existing file or folder (else the search covers the root)
local function names_a_path(args, root)
  for _, arg in ipairs(args) do
    if not arg:match('^%-') and vim.uv.fs_stat(vim.fs.joinpath(root, arg)) then
      return true
    end
  end
  return false
end

-- Runs `topos -m json --sort ARGS` in the root and shows what it finds
function M.run(args, title, opts)
  opts = opts or {}
  local path = cli()
  if not path then
    return
  end
  local root = opts.root or M.root()
  local cmd = { path, '-m', 'json', '--sort' }
  vim.list_extend(cmd, args)
  -- Searching the root unless a path was given (and never reading stdin)
  if not names_a_path(args, root) then
    table.insert(cmd, '.')
  end
  vim.system(cmd, { cwd = root, text = true }, function(result)
    vim.schedule(function()
      -- Exit code 1 only means nothing was found
      if result.code > 1 then
        local message = vim.trim(result.stderr or '')
        return vim.notify('topos: ' .. (message ~= '' and message or 'the search failed'), vim.log.levels.ERROR)
      end
      local items = {}
      for line in (result.stdout or ''):gmatch('[^\n]+') do
        table.insert(items, item(line, root))
      end
      M.show(title, items)
      if opts.done then
        opts.done(items)
      end
    end)
  end)
end

-- The reference under the cursor, written in full (`John 3:16`), or nil
function M.reference_at_cursor()
  local path = bin.path('cli')
  if not path then
    return nil
  end
  local line = vim.api.nvim_get_current_line()
  local col = vim.api.nvim_win_get_cursor(0)[2] + 1
  local result = vim.system({ path, '-m', 'json', '--text', line }, { text = true }):wait()
  for json in (result.stdout or ''):gmatch('[^\n]+') do
    local ok, m = pcall(vim.json.decode, json)
    if ok and m.column <= col and col <= m.end_column then
      return m.reference
    end
  end
  return nil
end

-- One kind of search for one passage (the reference under the cursor if `passage` is empty)
function M.passage(kind, passage)
  local spec = M.kinds[kind]
  passage = (passage and vim.trim(passage) ~= '') and vim.trim(passage) or M.reference_at_cursor()
  if not passage then
    return vim.notify('topos: give a passage, or put the cursor on a reference', vim.log.levels.WARN)
  end
  M.run({ spec.flag, passage }, spec.title:format(passage))
end

return M

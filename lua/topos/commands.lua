-- The :Topos* commands and their completions
local bin = require('topos.bin')
local search = require('topos.search')

local M = {}

-- Completes the CLI's options and their values, by asking the CLI (clap's completion protocol,
-- the same one shell completion uses)
function M.complete_flags(_, cmdline, pos)
  local path = bin.path('cli')
  if not path then
    return {}
  end
  local words = search.split(cmdline:sub(1, pos))
  table.remove(words, 1) -- the command
  if cmdline:sub(1, pos):match('%s$') then
    table.insert(words, '')
  end
  local cmd = { path, '--', 'topos' }
  vim.list_extend(cmd, words)
  local result = vim
    .system(cmd, {
      text = true,
      env = {
        COMPLETE = 'bash',
        _CLAP_IFS = '\n',
        _CLAP_COMPLETE_INDEX = tostring(#words),
        _CLAP_COMPLETE_COMP_TYPE = '9',
        _CLAP_COMPLETE_SPACE = 'true',
      },
    })
    :wait()
  local candidates = {}
  for candidate in (result.stdout or ''):gmatch('[^\n]+') do
    -- Values with spaces (Pauline Epistles) are quoted, so they stay one argument
    table.insert(candidates, candidate:find(' ') and ('"' .. candidate .. '"') or candidate)
  end
  return candidates
end

-- Completes a reference (books, then chapters, verses, and range ends), replacing only the
-- last word, since that is what the command line swaps in
function M.complete_passage(_, cmdline, pos)
  local path = bin.path('cli')
  if not path then
    return {}
  end
  local typed = cmdline:sub(1, pos):gsub('^%S+%s*', '')
  local result = vim.system({ path, '--complete', typed }, { text = true }):wait()
  local seen, candidates = {}, {}
  for candidate in (result.stdout or ''):gmatch('[^\n]+') do
    local last = typed:match('%s$') and candidate:match('%S+$') or candidate:match('%S+$')
    if last and not seen[last] then
      seen[last] = true
      table.insert(candidates, last)
    end
  end
  return candidates
end

local commands = {
  ToposSearch = {
    function(opts)
      local args = search.split(opts.args)
      search.run(args, #args > 0 and ('Bible references in ' .. opts.args) or 'Bible references')
    end,
    { nargs = '*', complete = 'dir', desc = 'Every Bible reference in the workspace (or the paths given)' },
  },
  ToposQuery = {
    function(opts)
      search.run(search.split(opts.args), 'topos ' .. opts.args)
    end,
    { nargs = '+', complete = M.complete_flags, desc = 'Search with the CLI\'s options, like -g Wisdom' },
  },
  ToposLspStart = {
    function()
      require('topos').lsp_start()
    end,
    { desc = 'Start topos-lsp for this buffer (whatever its file type), or restart it' },
  },
  ToposBuild = {
    function(opts)
      require('topos.build').build(opts.bang, function(ok)
        if ok then
          require('topos').start_lsp()
        end
      end)
    end,
    { bang = true, desc = 'Build topos-lsp and topos (! rebuilds)' },
  },
}

local passages = {
  ToposExplicitOverlap = 'explicit_overlap',
  ToposAnyOverlap = 'any_overlap',
  ToposExactOverlap = 'exact_overlap',
  ToposInside = 'inside',
  ToposExcludeOverlap = 'exclude_overlap',
}
for name, kind in pairs(passages) do
  commands[name] = {
    function(opts)
      search.passage(kind, opts.args)
    end,
    {
      nargs = '*',
      complete = M.complete_passage,
      desc = search.kinds[kind].title:format('{passage}') .. ' (the reference under the cursor by default)',
    },
  }
end

function M.create()
  for name, command in pairs(commands) do
    vim.api.nvim_create_user_command(name, command[1], command[2])
  end
end

return M

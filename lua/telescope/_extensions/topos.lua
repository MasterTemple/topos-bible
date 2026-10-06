-- Telescope pickers for topos: `:Telescope topos` (every reference), `:Telescope topos query
-- args=...`, and one per search kind, like `:Telescope topos explicit_overlap passage=John\ 3:16`
-- (the reference under the cursor if no passage is given)
local search = require('topos.search')

local exports = {
  search = function(opts)
    search.run({}, 'Bible references', { root = opts and opts.cwd })
  end,
  query = function(opts)
    local args = opts and opts.args or ''
    search.run(search.split(args), 'topos ' .. args, { root = opts and opts.cwd })
  end,
}
for kind in pairs(search.kinds) do
  exports[kind] = function(opts)
    search.passage(kind, opts and opts.passage)
  end
end
exports.topos = exports.search

return require('telescope').register_extension({ exports = exports })

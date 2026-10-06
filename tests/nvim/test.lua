-- Exercises the plugin in headless Neovim (run by tests/nvim/run.sh, which sets REPO and WORK)
local REPO, WORK = vim.env.REPO, vim.env.WORK
local out = {}
local function log(...) table.insert(out, table.concat(vim.tbl_map(tostring, {...}), " ")); vim.fn.writefile(out, WORK .. "/out.txt") end
local ok, err = xpcall(function()
vim.env.XDG_CONFIG_HOME = WORK .. "/config"; vim.env.XDG_CACHE_HOME = WORK .. "/cache"
vim.opt.rtp:prepend(REPO)
if vim.env.WITH_TELESCOPE then
  for _, p in ipairs({ "plenary.nvim", "telescope.nvim" }) do vim.opt.rtp:prepend((vim.env.PLUGINS or vim.fn.expand("~/.local/share/nvim/lazy")) .. "/" .. p) end
end
vim.cmd("runtime plugin/topos-bible.lua")
require("topos").setup({ cmd = { lsp = REPO .. "/target/debug/topos-lsp", cli = REPO .. "/target/debug/topos" }, auto_build = false })
vim.cmd("edit " .. WORK .. "/ws/open.txt")
vim.wait(5000, function() return #vim.lsp.get_clients({ bufnr = 0, name = "topos" }) > 0 end)
vim.wait(500)
local client = vim.lsp.get_clients({ bufnr = 0, name = "topos" })[1]
log("lsp", client and client.name, "hints", vim.lsp.inlay_hint.is_enabled({ bufnr = 0 }))
local telescope = vim.env.WITH_TELESCOPE ~= nil
local function results(cmd)
  local before = vim.fn.getqflist({ id = 0 }).id
  if type(cmd) == "function" then cmd() else vim.cmd(cmd) end
  if telescope then
    vim.wait(5000, function() return vim.bo.filetype == "TelescopePrompt" end)
    vim.wait(300)
    local picker = require("telescope.actions.state").get_current_picker(vim.api.nvim_get_current_buf())
    local title, n = picker.prompt_title, picker.manager:num_results()
    require("telescope.actions").close(vim.api.nvim_get_current_buf())
    vim.wait(100)
    return title, n
  end
  vim.wait(5000, function() return vim.fn.getqflist({ id = 0 }).id ~= before end)
  local title, n = vim.fn.getqflist({ title = 1 }).title, #vim.fn.getqflist()
  vim.cmd("cclose")
  vim.cmd("buffer " .. WORK .. "/ws/open.txt")
  return title, n
end
log("search", results("ToposSearch"))
log("query", results("ToposQuery -g Gospels --exclude-book Luke"))
log("explicit", results("ToposExplicitOverlap John 3:16"))
vim.api.nvim_win_set_cursor(0, { 1, 6 })
log("exact at cursor", results("ToposExactOverlap"))
log("any", results("ToposAnyOverlap John 3"))
log("inside", results("ToposInside John 3"))
log("complete flags", table.concat(vim.fn.getcompletion("ToposQuery -g Pau", "cmdline"), "|"))
log("complete ref", table.concat(vim.list_slice(vim.fn.getcompletion("ToposExplicitOverlap John 3:1", "cmdline"), 1, 3), "|"))
local ca = vim.lsp.util.make_range_params(0, client.offset_encoding); ca.context = { diagnostics = {} }
local actions = vim.lsp.buf_request_sync(0, "textDocument/codeAction", ca, 5000)
local action
for _, r in pairs(actions) do action = r.result[2] end
log("code action", action.title, results(function() client:exec_cmd(action.command, { bufnr = 0 }) end))
if telescope then
  log("telescope ext", results(function() require("telescope").extensions.topos.explicit_overlap({ passage = "Rom 8:28" }) end))
  log("telescope ext query", results(function() require("telescope").extensions.topos.query({ args = '-g "Pauline Epistles"' }) end))
end
vim.cmd("checkhealth topos")
local health = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
log("health ok", select(2, health:gsub("OK", "")), "errors", select(2, health:gsub("ERROR", "")))
end, debug.traceback)
if not ok then log("ERROR", err) end
vim.cmd("qa!")

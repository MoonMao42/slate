-- Exercise the actual filesystem and libuv backend in an isolated HOME.
local uv = vim.uv or vim.loop
local state = vim.env.SLATE_TEST_STATE
assert(os.remove(state)) -- loading with a missing state file must still arm the watcher
local slate = dofile(vim.env.SLATE_TEST_LOADER)
local changes = 0
vim.api.nvim_create_autocmd('ColorScheme', {
  pattern = 'slate-*',
  callback = function() changes = changes + 1 end,
})
local function settle(ms) vim.wait(ms, function() return false end) end
local function write(variant)
  local file = assert(io.open(state .. '.swap', 'w'))
  assert(file:write('return "' .. variant .. '"\n'))
  assert(file:close())
  assert(uv.fs_rename(state .. '.swap', state))
end
local function expect(variant)
  assert(vim.wait(2000, function() return vim.g.colors_name == 'slate-' .. variant end),
    'theme did not reload: ' .. variant)
end

write('nord')
expect('nord')
settle(150)
changes = 0
for i = 1, 12 do
  write(i % 2 == 0 and 'dracula' or 'catppuccin-mocha')
  settle(10)
end
expect('dracula')
settle(150)
assert(changes == 1, 'atomic-write burst was not debounced: ' .. changes)

assert(os.remove(state))
settle(200)
write('nord')
expect('nord')
vim.fn.writefile({ 'return "catppuccin-mocha"' }, state)
expect('catppuccin-mocha') -- in-place edits work as well as atomic replacement
settle(150)
local before = changes
vim.fn.writefile({ 'unrelated' }, state .. '.other')
settle(200)
assert(changes == before, 'unrelated directory event reapplied the colorscheme')

slate.stop()
write('dracula')
settle(200)
assert(vim.g.colors_name == 'slate-catppuccin-mocha', 'stop did not disable live reload')
for _ = 1, 3 do assert(slate.setup()) end
expect('dracula')
write('nord')
expect('nord')
slate.stop()
print('SLATE_NVIM_LIFECYCLE_OK')

-- Real Neovim API, deterministic libuv handles and callback delivery.
local real_uv = vim.uv or vim.loop
local real_schedule_wrap = vim.schedule_wrap
local real_notify = vim.notify
local timers, watchers, queued, warnings = {}, {}, {}, {}
local fail_watch = false
local fail_timer = false

local function handle(kind)
  local h = { closing = false }
  function h:start(target, _, callback)
    self.target, self.callback = target, callback
    if kind == 'watcher' and fail_watch then return nil, 'EACCES: fixture' end
    return 0
  end
  function h:stop() return 0 end
  function h:close()
    assert(not self.closing, 'handle closed twice')
    self.closing = true
  end
  function h:is_closing() return self.closing end
  table.insert(kind == 'timer' and timers or watchers, h)
  return h
end

vim.uv = setmetatable({
  new_timer = function()
    if fail_timer then return nil, 'ENOMEM: fixture' end
    return handle('timer')
  end,
  new_fs_event = function() return handle('watcher') end,
}, { __index = real_uv })
vim.schedule_wrap = function(callback)
  return function(...)
    local args = { ... }
    local count = select('#', ...)
    table.insert(queued, function() callback(unpack(args, 1, count)) end)
  end
end
vim.notify = function(message) table.insert(warnings, message) end

local function flush()
  while #queued > 0 do table.remove(queued, 1)() end
end
local function emit(name)
  watchers[#watchers].callback(nil, name or 'current_theme.lua', { change = true })
  flush()
end
local function open_count(handles)
  local count = 0
  for _, h in ipairs(handles) do if not h.closing then count = count + 1 end end
  return count
end

local slate = dofile(vim.env.SLATE_TEST_LOADER)
for _ = 1, 25 do
  local previous = timers[#timers]
  emit()
  assert(not previous or previous.closing, 'previous debounce timer remains open')
  assert(open_count(timers) == 1, 'more than one debounce timer is live')
end
assert(watchers[#watchers].target == vim.fn.fnamemodify(vim.env.SLATE_TEST_STATE, ':h'),
  'watch the parent directory, not the replaced state inode')

-- A stale timer may already be scheduled when a newer event cancels it.
timers[#timers].callback()
local stale_timer_callback = table.remove(queued, 1)
emit()
local newest = timers[#timers]
stale_timer_callback()
assert(not newest.closing, 'stale callback closed the newer timer')
newest.callback()
flush()
assert(open_count(timers) == 0, 'completed timer remains open')

-- Errors in a colorscheme/plugin hook cannot strand the completed timer.
local real_load = slate.load
slate.load = function() error('fixture colorscheme failure') end
emit()
timers[#timers].callback()
local ok, err = pcall(flush)
assert(not ok and tostring(err):find('fixture colorscheme failure', 1, true))
assert(open_count(timers) == 0, 'failed colorscheme left a timer open')
slate.load = real_load

emit()
timers[#timers].callback() -- queued reload from the previous setup
watchers[#watchers].callback(nil, 'current_theme.lua', {}) -- queued old watcher
slate.setup()
local timer_count = #timers
flush()
assert(#timers == timer_count, 'old setup callback restarted a timer')
for _ = 1, 20 do slate.setup() end
assert(open_count(watchers) == 1, 'repeated setup leaked watchers')
assert(#vim.api.nvim_get_autocmds({ event = 'VimLeavePre' }) == 1,
  'repeated setup accumulated exit callbacks')

timer_count = #timers
emit('unrelated-cache-file')
assert(#timers == timer_count, 'unrelated cache write triggered a reload')

fail_watch = true
ok, err = slate.setup()
assert(not ok and tostring(err):find('EACCES', 1, true), 'watch failure was hidden')
assert(open_count(watchers) == 0 and open_count(timers) == 0,
  'failed watcher startup leaked handles')
assert(#warnings == 1 and warnings[1]:find('EACCES', 1, true), 'missing actionable warning')
fail_watch = false
assert(slate.setup(), 'setup did not recover after watcher failure')

-- Failed initial loading, allocation, and asynchronous watch errors also release ownership.
slate.load = function() error('fixture setup failure') end
ok, err = pcall(slate.setup)
assert(not ok and tostring(err):find('fixture setup failure', 1, true))
assert(open_count(watchers) == 0 and open_count(timers) == 0)
slate.load = real_load
assert(slate.setup())
emit()
fail_timer = true
emit()
assert(open_count(timers) == 0, 'failed allocation retained the previous timer')
assert(warnings[#warnings]:find('ENOMEM', 1, true))
fail_timer = false
watchers[#watchers].callback('EIO: fixture', nil, {})
flush()
assert(open_count(watchers) == 0 and open_count(timers) == 0)
assert(warnings[#warnings]:find('EIO', 1, true))
assert(slate.setup())

emit()
timers[#timers].callback()
watchers[#watchers].callback(nil, 'current_theme.lua', {})
vim.api.nvim_exec_autocmds('VimLeavePre', {})
flush()
slate.stop() -- idempotent after exit cleanup
assert(open_count(watchers) == 0 and open_count(timers) == 0,
  'queued callback reopened a handle after exit cleanup')

vim.uv = real_uv
vim.schedule_wrap = real_schedule_wrap
vim.notify = real_notify
print('SLATE_NVIM_LIFECYCLE_OK')

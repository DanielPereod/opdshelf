-- Tests the OPDShelf KOReader plugin against stand-ins for the KOReader modules it uses.
-- Run from the repository root: luajit koreader/test/plugin_test.lua koreader/opdshelf.koplugin
-- (needs luajit and lua-cjson).
package.cpath = package.cpath .. ";/usr/lib/x86_64-linux-gnu/lua/5.1/?.so"
local cjson = require("cjson")
local plugin_dir = arg[1]
package.path = plugin_dir .. "/?.lua;" .. package.path

local log = {}
local function note(...) local t = {} for i = 1, select("#", ...) do t[#t+1] = tostring(select(i, ...)) end log[#log+1] = table.concat(t, " ") print("  >", log[#log]) end

local scheduled = {}
local http_responses = {}
local requests = {}
local online = true
local shown = {}

local function settingsStore()
    local data = {}
    return {
        readSetting = function(self, k, default) if data[k] == nil and default ~= nil then data[k] = default end return data[k] end,
        saveSetting = function(self, k, v) data[k] = v end,
        delSetting = function(self, k) data[k] = nil end,
        flush = function() end,
        data = data,
    }
end
local global_settings = settingsStore()
global_settings:saveSetting("device_id", "KOBO-ID")
_G.G_reader_settings = global_settings

local Widget = {}
function Widget:extend(o) o = o or {} setmetatable(o, self) self.__index = self return o end
function Widget:new(o) o = o or {} setmetatable(o, self) self.__index = self if o.init then o:init() end return o end

package.preload["ui/widget/container/widgetcontainer"] = function() return Widget end
package.preload["ui/widget/confirmbox"] = function() return { new = function(_, o) o.kind = "confirm" return o end } end
package.preload["ui/widget/infomessage"] = function() return { new = function(_, o) o.kind = "info" return o end } end
package.preload["datastorage"] = function() return { getSettingsDir = function() return "/settings" end } end
package.preload["device"] = function() return { model = "Kobo_test" } end
package.preload["ui/event"] = function() return { new = function(_, name, ...) return { name = name, args = { ... } } end } end
package.preload["json"] = function() return { encode = cjson.encode, decode = cjson.decode } end
local files = {}
package.preload["luasettings"] = function() return { open = function(_, path) files[path] = files[path] or settingsStore() return files[path] end } end
package.preload["ui/network/manager"] = function() return { isOnline = function() return online end } end
package.preload["ui/uimanager"] = function() return {
    show = function(_, w) shown[#shown+1] = w note("show", w.kind, w.text) end,
    scheduleIn = function(_, s, f) scheduled[#scheduled+1] = f end,
    nextTick = function(_, f) f() end,
    unschedule = function() end,
    setDirty = function() note("setDirty") end,
} end
package.preload["socket.http"] = function() return { request = function(req)
    local body = req.source and req.source() or nil
    requests[#requests+1] = { method = req.method, url = req.url, headers = req.headers, body = body }
    note("http", req.method, req.url, body or "")
    local r = table.remove(http_responses, 1) or { 200, "{}" }
    if r[1] == "offline" then return nil, "connection refused" end
    req.sink(r[2]) req.sink(nil)
    return 1, r[1], {}, "HTTP/1.1"
end } end
package.preload["logger"] = function() return { warn = function(...) note("warn", ...) end, dbg = function() end } end
package.preload["ltn12"] = function() return {
    source = { string = function(s) local done = false return function() if done then return nil end done = true return s end end },
    sink = { table = function(t) return function(chunk) if chunk then t[#t+1] = chunk end return 1 end end },
} end
package.preload["socket"] = function() return { skip = function(n, ...) return select(n + 1, ...) end } end
package.preload["socketutil"] = function() return { set_timeout = function() end, reset_timeout = function() end } end
package.preload["util"] = function() return { cleanupSelectedText = function(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end } end
package.preload["ffi/util"] = function() return { template = function(s, a) return (s:gsub("%%1", a)) end } end
package.preload["gettext"] = function() return function(s) return s end end

-- A book of words: xpointers are "/body/DocFragment[1]/body/p[1]/text().N" with N a word index
local words = {}
for i = 1, 200 do words[i] = "w" .. i end
local function xp(n) return "/body/DocFragment[1]/body/p[1]/text()." .. n end
local function idx(x) return tonumber(x:match("%.(%d+)$")) end
local page_first, page_size = 1, 20
local selection_drawn
local doc = {
    info = { has_pages = false, doc_height = 1000 },
    getPrevVisibleWordStart = function(_, x) local n = idx(x) return n > 1 and xp(n - 1) or nil end,
    getNextVisibleWordEnd = function(_, x) local n = idx(x) return n < #words and xp(n + 1) or nil end,
    getTextFromXPointers = function(_, a, b, draw) if draw then selection_drawn = { a, b } end local t = {} for i = idx(a), idx(b) - 1 do t[#t+1] = words[i] end return table.concat(t, " ") end,
    getPosFromXPointer = function(_, x) return idx(x) * 5 end,
    isXPointerInCurrentPage = function(_, x) local n = idx(x) return n >= page_first and n < page_first + page_size end,
    clearSelection = function() selection_drawn = nil note("clearSelection") end,
}
local doc_settings = settingsStore()
doc_settings:saveSetting("partial_md5_checksum", "DOCHASH")
local highlight_buttons = {}
local events = {}
local kosync = {
    settings = { custom_server = "https://books.example/kosync/", username = "admin", userkey = "KEY", auto_sync = false },
    getDocumentDigest = function() return "DOCHASH" end,
}
function kosync.getLastProgress(this) return xp(page_first) end
local ui = {
    document = doc, doc_settings = doc_settings, kosync = kosync,
    rolling = { getLastPercent = function() return 0.1 end },
    highlight = { addToHighlightDialog = function(_, id, fn) highlight_buttons[id] = fn end },
    handleEvent = function(_, e) events[#events+1] = e note("event", e.name, e.args[1]) end,
}

local OPDShelf = dofile(plugin_dir .. "/main.lua")
local p = OPDShelf:new{ ui = ui, dialog = {}, view = {}, document = doc }
local function check(cond, msg) if not cond then error("FAILED: " .. msg, 2) end print("ok - " .. msg) end

-- 1. Mark a selection
check(highlight_buttons["12_opdshelf_last_position"] ~= nil, "adds a button to the selection menu")
p:onReaderReady()
local hl = { ui = ui, selected_text = { pos0 = xp(12), pos1 = xp(14), text = " w12 w13 " }, onClose = function() note("dialog closed") end }
local button = highlight_buttons["12_opdshelf_last_position"](hl)
check(button.show_in_highlight_dialog_func() == true, "shown for reflowable books")
http_responses = { { 200, '{"document":"DOCHASH","timestamp":1}' } }
button.callback()
local put = cjson.decode(requests[#requests].body)
check(requests[#requests].url == "https://books.example/kosync/syncs/progress", "PUTs to the kosync endpoint")
check(requests[#requests].headers["x-auth-key"] == "KEY", "uses Progress sync's login")
check(put.manual == true and put.progress == xp(12) and put.device_id == "KOBO-ID" and put.document == "DOCHASH", "sends the selection start as a manual position")
check(put.context.selected == "w12 w13" and put.context.before:match("w11$") and put.context.after:match("^w12"), "sends the text around it")
check(put.timestamp and put.percentage == 0.06, "sends the time and percentage")
check(shown[#shown].text:match("Last position set"), "confirms it")
check(doc_settings:readSetting("opdshelf_mark") == xp(12), "remembers the mark for this book")

-- 2. Progress sync sends the mark while its page is on screen
check(kosync:getLastProgress() == xp(12), "Progress sync pushes the mark instead of the page top")
page_first = 21; p:onPageUpdate()
check(kosync:getLastProgress() == xp(21), "after turning the page it pushes the page again")
check(doc_settings:readSetting("opdshelf_mark") == nil, "and the mark is forgotten")

-- 3. Offline: queued, sent on reconnect
online = false
local n = #requests
button.callback()
check(#requests == n, "nothing sent while offline")
check(shown[#shown].text:match("when the device is online"), "says it will be sent later")
online = true
p:onNetworkConnected(); table.remove(scheduled)()
check(#requests == n + 1 and cjson.decode(requests[#requests].body).manual, "sent once online")
check(next(files["/settings/opdshelf.lua"]:readSetting("pending")) == nil, "and removed from the queue")
-- Server unreachable while "online": kept
http_responses = { { "offline" } }
button.callback()
check(next(files["/settings/opdshelf.lua"]:readSetting("pending")) ~= nil, "kept when the server cannot be reached")
http_responses = { { 200, "{}" } }
p:flush(false)
check(next(files["/settings/opdshelf.lua"]:readSetting("pending")) == nil, "sent on the next try")

-- 4. Opening a book marked on another device
page_first = 1
local p2 = OPDShelf:new{ ui = ui, dialog = {}, view = {}, document = doc }
doc_settings:delSetting("opdshelf_mark")
p2:onReaderReady()
http_responses = { { 200, cjson.encode({ manual = true, progress = xp(50), device = "Firefox on Linux", device_id = "WEB", context = { selected = "w50 w51 w52" } }) } }
table.remove(scheduled)()
check(shown[#shown].kind == "confirm" and shown[#shown].text:match("Firefox on Linux"), "offers to go to the point set on another device")
shown[#shown].ok_callback()
check(events[#events].name == "GotoXPointer" and events[#events].args[1] == xp(50), "goes there")
page_first = 41; p2:onPageUpdate()
check(selection_drawn and selection_drawn[1] == xp(50) and selection_drawn[2] == xp(53), "highlights the marked words")
check(kosync:getLastProgress() == xp(50), "and keeps the point as the position while there")
table.remove(scheduled)()
check(selection_drawn == nil, "the highlight goes away")

-- 5. Own mark or automatic positions are left alone
local p3 = OPDShelf:new{ ui = ui, dialog = {}, view = {}, document = doc }
p3:onReaderReady()
local before = #shown
http_responses = { { 200, cjson.encode({ manual = false, progress = xp(90), device = "Web", device_id = "WEB" }) } }
table.remove(scheduled)()
check(#shown == before, "automatic positions are left to Progress sync")
print("all good")

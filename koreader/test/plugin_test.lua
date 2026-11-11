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

local function dialog(kind) return function() return { new = function(_, o) o.kind = kind return o end } end end
package.preload["ui/widget/container/widgetcontainer"] = function() return Widget end
package.preload["ui/widget/confirmbox"] = dialog("confirm")
package.preload["ui/widget/multiconfirmbox"] = dialog("choose")
package.preload["ui/widget/infomessage"] = dialog("info")
package.preload["ui/widget/buttondialog"] = dialog("buttons")
package.preload["ui/widget/inputdialog"] = dialog("input")
package.preload["ui/widget/multiinputdialog"] = dialog("form")
package.preload["ui/widget/notification"] = function() return {
    SOURCE_ALWAYS_SHOW = 1,
    notify = function(_, text) shown[#shown+1] = { kind = "notification", text = text } note("notify", text) end,
} end
package.preload["dispatcher"] = function() return { registerAction = function() end } end
package.preload["datastorage"] = function() return { getSettingsDir = function() return "/settings" end, getDataDir = function() return "/data" end } end
package.preload["device"] = function() return { model = "Kobo_test" } end
package.preload["ui/event"] = function() return { new = function(_, name, ...) return { name = name, args = { ... } } end } end
package.preload["json"] = function() return { encode = cjson.encode, decode = cjson.decode } end
local files = {}
package.preload["luasettings"] = function() return { open = function(_, path) files[path] = files[path] or settingsStore() return files[path] end } end
package.preload["ui/network/manager"] = function() return {
    isOnline = function() return online end,
    runWhenOnline = function(_, f) f() end,
} end
package.preload["ui/uimanager"] = function() return {
    show = function(_, w) shown[#shown+1] = w note("show", w.kind, w.text) end,
    close = function(_, w) note("close", w.kind) end,
    scheduleIn = function(_, s, f) scheduled[#scheduled+1] = { s = s, f = f } end,
    nextTick = function(_, f) f() end,
    unschedule = function(_, f) for i = #scheduled, 1, -1 do if scheduled[i].f == f then table.remove(scheduled, i) end end end,
    setDirty = function() end,
} end
-- Runs what is scheduled (once)
local function runScheduled()
    local list = scheduled
    scheduled = {}
    for _, item in ipairs(list) do item.f() end
end
local function respond(code, body) http_responses[#http_responses+1] = { code, type(body) == "table" and cjson.encode(body) or body } end
package.preload["socket.http"] = function() return { request = function(req)
    local body = req.source and req.source() or nil
    requests[#requests+1] = { method = req.method, url = req.url, headers = req.headers, body = body and cjson.decode(body) }
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
package.preload["mime"] = function() return { b64 = function(s) return "B64(" .. s .. ")" end } end
-- The live connection never connects in these tests
package.preload["socket"] = function() return { tcp = function() return {
    settimeout = function() end, connect = function() return nil, "timeout" end, close = function() end,
} end } end
package.preload["socketutil"] = function() return { set_timeout = function() end, reset_timeout = function() end } end
package.preload["util"] = function() return {
    cleanupSelectedText = function(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end,
    trim = function(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end,
} end
package.preload["ffi/util"] = function() return { template = function(s, ...)
    local args = { ... }
    return (s:gsub("%%(%d)", function(i) return tostring(args[tonumber(i)]) end))
end } end
package.preload["gettext"] = function() return function(s) return s end end

-- A book of words: xpointers are "/body/DocFragment[1]/body/p[1]/text().N" with N a word index
local words = {}
for i = 1, 400 do words[i] = "w" .. i end
local function xp(n) return "/body/DocFragment[1]/body/p[1]/text()." .. n end
local function idx(x) return tonumber(x:match("%.(%d+)$")) end
local page_first, page_size = 1, 20
local selection_drawn
local doc = {
    info = { has_pages = false, doc_height = 1000 },
    file = "/books/book.epub",
    getPrevVisibleWordStart = function(_, x) local n = idx(x) return n > 1 and xp(n - 1) or nil end,
    getNextVisibleWordEnd = function(_, x) local n = idx(x) return n < #words and xp(n + 1) or nil end,
    getTextFromXPointers = function(_, a, b, draw) if draw then selection_drawn = { a, b } end local t = {} for i = idx(a), idx(b) - 1 do t[#t+1] = words[i] end return table.concat(t, " ") end,
    getPosFromXPointer = function(_, x) return idx(x) * 5 end,
    isXPointerInDocument = function(_, x) return idx(x) ~= nil and idx(x) <= #words end,
    isXPointerInCurrentPage = function(_, x) local n = idx(x) return n >= page_first and n < page_first + page_size end,
    clearSelection = function() selection_drawn = nil note("clearSelection") end,
}
local doc_settings = settingsStore()
doc_settings:saveSetting("partial_md5_checksum", "DOCHASH")
local highlight_buttons = {}
local events = {}
local ui
ui = {
    document = doc, doc_settings = doc_settings,
    rolling = {
        getLastProgress = function() return xp(page_first) end,
        getLastPercent = function() return page_first / #words end,
    },
    highlight = { addToHighlightDialog = function(_, id, fn) highlight_buttons[id] = fn end },
    handleEvent = function(_, e)
        events[#events+1] = e
        note("event", e.name, e.args[1])
        if e.name == "GotoXPointer" then
            page_first = idx(e.args[1])
            -- KOReader tells plugins the page changed
            for _, plugin in ipairs(ui.plugins or {}) do plugin:onPageUpdate() end
        end
    end,
}

-- Progress sync pointed to OPDShelf before
files["/settings/kosync.lua"] = settingsStore()
files["/settings/kosync.lua"]:saveSetting("settings", { custom_server = "https://books.example/kosync/", username = "admin", userkey = "KEY" })

local OPDShelf = dofile(plugin_dir .. "/main.lua")
local state = function() return files["/settings/opdshelf.lua"] end
local function check(cond, msg) if not cond then error("FAILED: " .. msg, 2) end print("ok - " .. msg) end
local function last() return requests[#requests] end
local function open()
    local p = OPDShelf:new{ ui = ui, dialog = {}, view = {} }
    ui.plugins = { p }
    p:onReaderReady()
    return p
end
local function remote(device, n, age, extra)
    local r = { progress = xp(n), percentage = n / #words, device = device, device_id = device, timestamp = os.time() - (age or 0) }
    for k, v in pairs(extra or {}) do r[k] = v end
    return r
end

-- 1. Account
local p = OPDShelf:new{ ui = ui, dialog = {}, view = {} }
check(state():readSetting("server") == "https://books.example", "takes the server from Progress sync, without /kosync")
check(state():readSetting("username") == "admin", "and its username")
check(not p:ready(), "but needs the password")
state():saveSetting("password", "secret")
state():saveSetting("live", false)
p:makeClient()
check(p:ready(), "ready with a password")

-- 2. Set as last position
p = open()
check(#scheduled == 1, "syncs once the book is open")
respond(200, { current = remote("WEB", 1, 5000) }) -- nothing newer: stays
runScheduled()
check(last().method == "GET" and last().url == "https://books.example/api/progress/DOCHASH", "asks the server where the book is")
check(last().headers.Authorization == "Basic B64(admin:secret)", "with the server's own login")
local hl = { ui = ui, selected_text = { pos0 = xp(12), pos1 = xp(14), text = " w12 w13 " }, onClose = function() end }
local button = highlight_buttons["12_opdshelf_last_position"](hl)
check(button.show_in_highlight_dialog_func() == true, "selection button for reflowable books")
respond(200, { accepted = true, current = remote("KOBO-ID", 12), server_time = os.time() })
button.callback()
local put = last().body
check(last().method == "PUT" and last().url == "https://books.example/api/progress/DOCHASH", "PUTs to OPDShelf's own API")
check(put.manual == true and put.progress == xp(12) and put.device_id == "KOBO-ID" and put.device == "Kobo_test", "sends the selection start as a manual position")
check(put.context.selected == "w12 w13" and put.context.before:match("w11$") and put.context.after:match("^w12"), "with the text around it")
check(put.timestamp and put.sent_at and put.percentage == 0.06, "and the time it was read and sent")
check(shown[#shown].text:match("Last position set"), "confirms it")

-- 3. Turning pages sends the position a few seconds later
page_first = 21; p:onPageUpdate()
check(doc_settings:readSetting("opdshelf_mark") == nil, "leaving the marked page forgets the mark")
check(#scheduled == 1 and scheduled[1].s == 3, "schedules sending the position")
page_first = 41; p:onPageUpdate()
check(#scheduled == 1, "only once while turning pages")
respond(200, { accepted = true, current = remote("KOBO-ID", 41) })
runScheduled()
check(last().body.progress == xp(41) and not last().body.manual, "sends the new page")
check(not doc_settings:readSetting("opdshelf_sync").moved, "and the server agrees")

-- 4. Offline: kept, sent when back online
online = false
page_first = 61; p:onPageUpdate()
local n = #requests
runScheduled()
check(#requests == n, "nothing sent while offline")
check(state():readSetting("pending").DOCHASH.progress == xp(61), "kept in the queue")
online = true
respond(200, { accepted = true, current = remote("KOBO-ID", 61) })
respond(200, { current = remote("KOBO-ID", 61) })
p:onNetworkConnected(); runScheduled()
check(requests[n + 1].method == "PUT" and requests[n + 1].body.progress == xp(61), "sent once online")
check(next(state():readSetting("pending")) == nil, "and removed from the queue")
p:onCloseDocument()

-- 5. Opening the book when another device went further: goes there
page_first = 61
p = open()
respond(200, { current = remote("Pixel", 100) })
runScheduled()
check(events[#events].name == "GotoXPointer" and events[#events].args[1] == xp(100), "goes to where the other device is")
check(shown[#shown].kind == "notification" and shown[#shown].text:match("Pixel"), "and says from which device")
check(#scheduled == 0, "moving there is not sent back as reading")
p:onCloseDocument()

-- 6. Another device behind: asks
p = open()
respond(200, { current = remote("Web", 80) })
runScheduled()
check(shown[#shown].kind == "confirm" and shown[#shown].text:match("Behind on Web"), "asks before going back")
respond(200, { accepted = true, current = remote("KOBO-ID", 100) })
shown[#shown].cancel_callback()
check(last().method == "PUT" and last().body.progress == xp(100), "staying makes this device's position the latest")
p:onCloseDocument()

-- 7. Read here offline, and further on another device meanwhile: lets you choose
p = open()
respond(200, { current = remote("KOBO-ID", 100) })
runScheduled()
online = false
page_first = 121; p:onPageUpdate(); runScheduled()
p:onCloseDocument()
online = true
p = open()
-- The queued page is sent, but the phone read later
respond(200, { accepted = false, current = remote("Phone", 150) })
runScheduled()
check(shown[#shown].kind == "choose" and shown[#shown].text:match("Phone"), "both read since they last synced: asks where to continue")
local n2 = #requests
shown[#shown].choice2_callback()
check(events[#events].args[1] == xp(150), "choosing the other device goes there")
check(#requests == n2, "without sending anything")
-- The same choice the other way round
online = false
page_first = 170; p:onPageUpdate(); runScheduled()
online = true
respond(200, { accepted = false, current = remote("Phone", 180, 0) })
p:flush()
check(shown[#shown].kind == "choose", "asks again for a new conflict")
respond(200, { accepted = true, current = remote("KOBO-ID", 170) })
shown[#shown].choice1_callback()
check(last().method == "PUT" and last().body.progress == xp(170) and last().body.timestamp >= os.time() - 1, "choosing here sends it as read now, so it wins")

-- 8. Live updates while the book is open
local before = #shown
p:onStreamEvent("progress", cjson.encode({ document = "DOCHASH", current = remote("KOBO-ID", 170) }))
check(#shown == before, "its own position coming back is ignored")
p:onStreamEvent("progress", cjson.encode({ document = "OTHER", current = remote("Phone", 190) }))
check(#shown == before, "other books are ignored")
p.last_move = os.time()
p:onStreamEvent("progress", cjson.encode({ document = "DOCHASH", current = remote("Phone", 190) }))
check(shown[#shown].kind == "confirm" and shown[#shown].text:match("Further ahead on Phone"), "while reading, never moves the page: asks")
shown[#shown].ok_callback()
check(events[#events].args[1] == xp(190), "and goes there on Go")
p.last_move = os.time() - 600
p:onStreamEvent("progress", cjson.encode({ document = "DOCHASH", current = remote("Phone", 230) }))
check(events[#events].args[1] == xp(230), "when the device is just lying open, follows the other one by itself")
respond(200, { current = remote("Phone", 260) })
p:onStreamEvent("resync", "{}")
check(events[#events].args[1] == xp(260), "after missing events it asks the server again")

-- 9. A point set by hand on another device
p.last_move = os.time() - 600
page_first = 1
p:onStreamEvent("progress", cjson.encode({ document = "DOCHASH",
    current = remote("Firefox", 50, 0, { manual = true, context = { selected = "w50 w51 w52" } }) }))
check(shown[#shown].kind == "confirm" and shown[#shown].text:match("A point to continue this book from was set on Firefox"), "offers to go to a point set on another device")
shown[#shown].ok_callback()
check(events[#events].name == "GotoXPointer" and events[#events].args[1] == xp(50), "goes there")
check(selection_drawn and selection_drawn[1] == xp(50) and selection_drawn[2] == xp(53), "and highlights the marked words")
check(p:position() == xp(50), "the point is the position while it is on screen")
runScheduled()
check(selection_drawn == nil, "the highlight goes away")
p:onCloseDocument()

-- 10. Live connection: only while live sync is on and the book is open
state():saveSetting("live", true)
p = open()
respond(200, { current = remote("KOBO-ID", 50) })
runScheduled()
check(p.stream ~= nil and p.stream.url == "https://books.example/api/progress/events?documents=DOCHASH", "opens the live connection for this book")
check(p.stream.headers.Authorization == "Basic B64(admin:secret)", "with the login")
p:onSuspend()
check(p.stream == nil, "closes it when the device sleeps")
p:onCloseDocument()
print("all good")

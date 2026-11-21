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
    findAllText = function(_, pattern)
        local hits = {}
        for i = 1, #words do
            local t = {}
            for j = i, math.min(#words, i + 5) do t[#t+1] = words[j] end
            if table.concat(t, " "):sub(1, #pattern) == pattern then hits[#hits+1] = { start = xp(i), ["end"] = xp(i + 1) } end
        end
        return hits
    end,
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
    annotation = { annotations = {}, addItem = function(self, item)
        item.datetime = "1999-01-01 00:00:00" -- KOReader stamps new items with now
        table.insert(self.annotations, item)
        return #self.annotations
    end },
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
-- Highlights are tested on their own, further down
state():saveSetting("sync_notes", false)
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
check(last().body.context.after:match("^w41 w42") and last().body.context.before:match("w40$"), "with the text there, so other readers can check it")
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

-- 5b. The xpointer leads to other words than the ones saved with it (another
-- reader built the chapter otherwise): goes where those words are
page_first = 100
p = open()
respond(200, { current = remote("Pixel", 300, 0, { context = { before = "w148 w149", after = "w150 w151 w152 w153" } }) })
runScheduled()
check(events[#events].name == "GotoXPointer" and events[#events].args[1] == xp(150), "finds the saved text when the xpointer leads elsewhere")
p:onCloseDocument()
page_first = 100
p = open()
respond(200, { current = remote("Pixel", 200, 0, { context = { after = "w200 w201 w202" } }) })
runScheduled()
check(events[#events].args[1] == xp(200), "and keeps the xpointer when its text matches")
p:onCloseDocument()
page_first = 100

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
check(p.stream ~= nil and p.stream.url:match("^https://books.example/api/progress/events%?documents=DOCHASH&device_id=KOBO%-ID&device=.+") ~= nil, "opens the live connection for this book, saying which device listens")
check(p.stream.headers.Authorization == "Basic B64(admin:secret)", "with the login")
p:onSuspend()
check(p.stream == nil, "closes it when the device sleeps")
p:onCloseDocument()

-- 10b. Opening a book is not reading
state():saveSetting("live", false)
page_first = 100
p = open()
respond(200, { current = remote("KOBO-ID", 100) })
runScheduled()
local sync_state = p:docState()
local three_days_ago = os.time() - 3 * 86400
sync_state.read_at, sync_state.moved = three_days_ago, false
-- KOReader says the page changed on opening, laying out and drawing dialogs
p:onPageUpdate(); p:onPosUpdate(); p:onPageUpdate()
check(#scheduled == 0 and not sync_state.moved and sync_state.read_at == three_days_ago and p.last_move == nil,
    "opening a book, or drawing its page again, is not reading")
-- Read here earlier while offline, and on a phone meanwhile
sync_state.moved = true
local n4 = #requests
p:handleRemote(remote("Phone", 90), "open")
check(shown[#shown].kind == "choose" and shown[#shown].text:find("Here: 25 %, 3 days ago", 1, true),
    "the conflict says when it was really read here, not \"just now\"")
p:onPageUpdate()
shown[#shown].cancel_callback()
check(#requests == n4 and #scheduled == 0 and sync_state.read_at == three_days_ago, "seeing it and cancelling sends nothing and changes no time")
page_first = 101; p:onPageUpdate()
check(#scheduled == 1 and p.last_move and sync_state.read_at >= os.time() - 1, "turning a page is reading")
p:onCloseDocument()
scheduled = {}


-- 11. Highlights and notes
state():saveSetting("live", false)
state():saveSetting("sync_notes", nil)
local annotations = ui.annotation.annotations
p = open()
respond(200, { current = remote("KOBO-ID", 50) })
local web_note = { id = "web1", text = "w30 w31 w32", note = "From the browser", color = "green", style = "lighten",
    pos0 = xp(30), pos1 = xp(33), chapter = "One", percentage = 0.075, created = os.time() - 100, updated = os.time() - 50,
    cfi = "epubcfi(/6/2!/4/2,/1:0,/1:9)", device = "Firefox", device_id = "web" }
respond(200, { annotations = { web_note, { id = "gone", deleted = true, updated = os.time() } } })
runScheduled()
check(last().method == "GET" and last().url == "https://books.example/api/annotations/DOCHASH", "gets the book's highlights after opening it")
check(#annotations == 1 and annotations[1].opdshelf_id == "web1" and annotations[1].note == "From the browser"
    and annotations[1].color == "green" and annotations[1].pos0 == xp(30), "adds the highlight made in the browser")
check(annotations[1].datetime ~= "1999-01-01 00:00:00", "keeping when it was made")
check(events[#events].name == "AnnotationsModified", "and tells KOReader")
local n3 = #requests
p:onAnnotationsModified()
runScheduled()
check(#requests == n3, "a highlight that came from the server is not sent back")

-- Highlighted here
annotations[2] = { datetime = os.date("%Y-%m-%d %H:%M:%S"), text = "w50 w51", pos0 = xp(50), pos1 = xp(52),
    color = "red", drawer = "underscore", chapter = "Two", page = xp(50) }
p:onAnnotationsModified()
check(scheduled[#scheduled].s == 2, "sends new highlights a moment later")
respond(200, { annotations = { web_note, { id = "placeholder" } } })
runScheduled()
local sent = last().body
check(last().method == "PUT" and last().url == "https://books.example/api/annotations/DOCHASH", "PUTs them")
check(#sent.annotations == 1 and sent.annotations[1].id:match("^kKOBOID") and sent.annotations[1].color == "red"
    and sent.annotations[1].style == "underscore" and sent.annotations[1].pos0 == xp(50) and sent.annotations[1].percentage == 0.25,
    "with an id of their own, colour, style, place and how far in the book")
check(sent.device_id == "KOBO-ID" and sent.sent_at, "saying which device sends them, and when")
local mine = sent.annotations[1]
mine.updated = os.time()

-- A note added to the browser's highlight
annotations[1].note = "Added on the Kobo"
p:onAnnotationsModified()
mine.created = mine.created or os.time()
respond(200, { annotations = { web_note, mine } })
runScheduled()
local function sentById(id) for _, a in ipairs(last().body.annotations) do if a.id == id then return a end end end
check(sentById("web1") and sentById("web1").note == "Added on the Kobo", "sends a changed note")
check(sentById("web1").cfi == web_note.cfi, "keeping what the web reader needs to find it")
check(sentById(mine.id), "and again what the server did not confirm")

-- Deleted here, while offline
online = false
table.remove(annotations, 1)
p:onAnnotationsModified()
runScheduled()
online = true
respond(200, { current = remote("KOBO-ID", 50) })
respond(200, { annotations = { web_note, mine } })
respond(200, { annotations = { mine } })
p:onNetworkConnected(); runScheduled()
local put = last().body.annotations
check(last().method == "PUT" and #put == 1 and put[1].id == "web1" and put[1].deleted == true,
    "a deletion made offline goes out once online, and the server's copy does not bring it back")
check(#annotations == 1, "so it stays deleted here")

-- Live: changed and deleted on another device
p:onStreamEvent("annotations", cjson.encode({ document = "DOCHASH", annotations = {
    { id = mine.id, text = mine.text, pos0 = mine.pos0, pos1 = mine.pos1, color = "blue", style = "lighten",
      note = "Edited on the phone", updated = os.time() + 10, device_id = "phone" } } }))
check(annotations[1].color == "blue" and annotations[1].note == "Edited on the phone" and annotations[1].drawer == "lighten",
    "a change made on another device arrives live")
local new_one = { id = "phone2", text = "w70 w71", pos0 = xp(70), pos1 = xp(72), color = "yellow", updated = os.time(), device_id = "phone" }
p:onStreamEvent("annotations", cjson.encode({ document = "DOCHASH", annotations = { new_one } }))
check(#annotations == 2 and shown[#shown].text:match("A highlight arrived"), "and so does a new one, with a note on screen")
local lost = { id = "lost", text = "not in this book", pos0 = "/body/DocFragment[9]/p", pos1 = "/body/DocFragment[9]/p[2]", updated = os.time() }
p:onStreamEvent("annotations", cjson.encode({ document = "DOCHASH", annotations = { lost } }))
check(#annotations == 2, "a highlight whose text is not in this copy is left out")
p:onStreamEvent("annotations", cjson.encode({ document = "DOCHASH", annotations = { { id = "phone2", deleted = true, updated = os.time() + 20 } } }))
check(#annotations == 1 and annotations[1].opdshelf_id == mine.id, "a deletion made elsewhere removes it here")
n3 = #requests
p:onAnnotationsModified(); runScheduled()
check(#requests == n3, "nothing to send back after applying them")
p:onCloseDocument()

-- 12. The home screen as KOReader's start screen
local start_with = { sub_item_table = { { text = "file browser" } }, text_func = function() return "Start with: file browser" end }
local menu_items = { start_with = start_with }
p:addToMainMenu(menu_items)
p:addToMainMenu(menu_items)
check(#start_with.sub_item_table == 2 and start_with.sub_item_table[2].text == "OPDShelf home", "adds the home screen to Start with, once")
check(start_with.text_func() == "Start with: file browser", "keeps KOReader's text for its own choices")
start_with.sub_item_table[2].callback()
check(global_settings:readSetting("start_with") == "opdshelf" and start_with.sub_item_table[2].checked_func(), "can be chosen")
check(start_with.text_func() == "Start with: OPDShelf home", "and says so")
check(menu_items.opdshelf.sub_item_table_func()[1].text == "OPDShelf home", "the plugin's menu opens it")
start_with.sub_item_table[1].callback = function() global_settings:saveSetting("start_with", "filemanager") end
start_with.opdshelf = nil
p:addStartWith(menu_items)
start_with.sub_item_table[1].callback()
check(not p:launcherEnabled(), "choosing another start screen turns it off")
state():saveSetting("launcher", nil)
check(p:launcherEnabled(), "it is on by default")
global_settings:saveSetting("start_with", "last")
check(not p:launcherEnabled(), "unless KOReader starts with something else")
p:setLauncher(true)
check(p:launcherEnabled() and global_settings:readSetting("start_with") == "opdshelf", "and can be turned on")
-- The file browser's home button and Start with menu
local FakeFM = { onHome = function() return "home folder" end }
local FakeMenu = { getStartWithMenuTable = function() return { sub_item_table = { { text = "file browser", callback = function() end } } } end }
package.loaded["apps/filemanager/filemanager"] = FakeFM
package.loaded["apps/filemanager/filemanagermenu"] = FakeMenu
p:patchFileManager()
p:patchFileManager()
local home_shown = 0
local fm_plugin = { launcherEnabled = function() return true end, showHome = function() home_shown = home_shown + 1 end }
check(FakeFM.onHome({ opdshelf = fm_plugin }) == true and home_shown == 1, "the home button opens the home screen")
fm_plugin.launcherEnabled = function() return false end
check(FakeFM.onHome({ opdshelf = fm_plugin }) == "home folder", "or the home folder when it is off")
local menu = FakeMenu.getStartWithMenuTable()
check(#menu.sub_item_table == 2 and menu.sub_item_table[2].text == "OPDShelf home", "Start with lists it however its menu is built")
global_settings:saveSetting("language", "es")
local _es = dofile(plugin_dir .. "/opdshelf_i18n.lua")
check(_es("Home") == "Inicio" and _es("Not in the catalog") == "Not in the catalog", "speaks Spanish, and English for the rest")
global_settings:saveSetting("language", "en")
check(_es("Home") == "Home", "and English otherwise")
-- Setup from the panel, updates and status
local Update = dofile(plugin_dir .. "/opdshelf_update.lua")
local tmp = os.tmpname()
os.remove(tmp)
local dir = tmp .. "/plugins/opdshelf.koplugin"
os.execute("mkdir -p '" .. dir .. "'")
local function write(path, text) local f = assert(io.open(path, "w")) f:write(text) f:close() end
local function exists(path) local f = io.open(path) if f then f:close() return true end return false end
local function read(path) local f = io.open(path) if not f then return nil end local t = f:read("*a") f:close() return t end
write(dir .. "/_meta.lua", 'return {\n    name = "opdshelf",\n    version = 202601010000,\n}')
local setup = cjson.encode({ server = "http://new.example", username = "dani", password = "pw" })
write(dir .. "/opdshelf_setup.json", setup)
local q = OPDShelf:new{ ui = ui, dialog = {}, view = {}, path = dir }
check(state():readSetting("server") == "https://books.example", "an account already set up stays")
check(not exists(dir .. "/opdshelf_setup.json"), "and the setup file is removed, as it holds the password")
state():saveSetting("server", nil)
write(dir .. "/opdshelf_setup.json", setup)
q = OPDShelf:new{ ui = ui, dialog = {}, view = {}, path = dir }
check(state():readSetting("server") == "http://new.example" and state():readSetting("username") == "dani"
    and state():readSetting("password") == "pw" and q:ready(), "a plugin from the panel comes set up")
check(q:installedVersion() == 202601010000 and Update.describe(q:installedVersion()) == "2026-01-01 00:00", "knows its version")
-- KOReader's plugin loader copies the _meta.lua fields but name onto the plugin: none may hide a method
for _, key in ipairs({ "version", "fullname", "description" }) do
    check(OPDShelf[key] == nil, "no method is called like the _meta.lua field " .. key)
end

shown = {}
respond(200, { koreader = { version = 202601010000 } })
q:checkForUpdate(true)
check(last().url == "http://new.example/apps/versions.json" and shown[#shown].text == "OPDShelf is up to date.", "says when it is up to date")
respond(200, { koreader = { version = 202602020000 } })
q:checkForUpdate(false)
check(shown[#shown].kind == "confirm" and shown[#shown].text:find("2026-02-02 00:00", 1, true), "offers a newer version")

local fs = {
    exists = exists,
    mkdir = function(path) os.execute("mkdir -p '" .. path .. "'") end,
    purge = function(path) os.execute("rm -rf '" .. path .. "'") end,
    unpack = function(zip, to)
        if read(zip) ~= "ZIP" then return false, "not a zip" end
        os.execute("mkdir -p '" .. to .. "/opdshelf.koplugin'")
        write(to .. "/opdshelf.koplugin/main.lua", "return {}")
        write(to .. "/opdshelf.koplugin/_meta.lua", "return { version = 202602020000 }")
        write(to .. "/opdshelf.koplugin/opdshelf_setup.json", "{}")
        return true
    end,
}
local fake = { download = function(_, url, path) write(path, url == Update.URL and "ZIP" or "") return true end }
check(Update.install(fake, dir, fs) == true and Update.localVersion(dir) == 202602020000, "installs the update in place")
check(not exists(dir .. "/opdshelf_setup.json") and not exists(dir .. ".old/main.lua")
    and not exists(tmp .. "/plugins/opdshelf-update.zip") and not exists(tmp .. "/plugins/opdshelf-update/main.lua"), "and leaves nothing behind")
local broken = { download = function(_, _url, path) write(path, "HTML") return true end }
local ok, why = Update.install(broken, dir, fs)
check(not ok and why == "not a zip" and Update.localVersion(dir) == 202602020000, "a bad download leaves the plugin as it was")

shown = {}
state():saveSetting("pending", { A = {}, B = {} })
respond(401, "")
q:noteError("Something broke")
q:showStatus()
local status = shown[#shown].title
check(status:find("Version: 2026-02-02 00:00", 1, true) and status:find("Connection: wrong username or password", 1, true)
    and status:find("Positions waiting to be sent: 2", 1, true) and status:find("Last error: Something broke", 1, true), "the status screen says how things are")
state():saveSetting("pending", nil)
os.execute("rm -rf '" .. tmp .. "'")

-- The menu: where to go, this book, the settings in one submenu, the status
local menu = q:menuItems()
local labels = {}
for i, item in ipairs(menu) do labels[i] = item.text or item.text_func() end
check(table.concat(labels, "|") == "OPDShelf home|Library|Search the library|This book|Settings|Status and updates",
    "the menu has three groups and the status")
check(#menu[4].sub_item_table == 3 and menu[4].sub_item_table[1].text == "Sync this book now", "what to do with the book open")
local settings = menu[5].sub_item_table_func()
local names = {}
for _i, item in ipairs(settings) do names[#names + 1] = item.text or item.text_func() end
names = table.concat(names, "|")
check(names:find("Server: ", 1, true) and names:find("Download folder: ", 1, true) and names:find("Show the home screen at start", 1, true)
    and names:find("Live sync while reading", 1, true), "and every setting in one place")
local heard
settings = q:settingsItems(function(what) heard = what end)
local chooser = q.chooseDownloadDir
q.chooseDownloadDir = function(_self, done) done() end
settings[3].callback()
q.chooseDownloadDir = chooser
check(heard == "folder", "the home screen hears when a setting it shows changed")

-- 14. Positions are only dropped when OPDShelf itself refuses them
doc_settings:delSetting("opdshelf_sync")
state():saveSetting("pending", nil)
online = true
page_first = 120
p = open()
respond(200, { current = remote("KOBO-ID", 120) })
runScheduled()
respond(404, "<html>Not found</html>")
p:push{ force = true }
check(state():readSetting("pending").DOCHASH ~= nil, "a 404 page from a proxy keeps the position in the queue")
respond(200, "<html>Sign in to the Wi-Fi</html>")
p:flush()
check(state():readSetting("pending").DOCHASH ~= nil, "so does a captive portal answering 200 with a page")
respond(403, "")
p:flush()
check(state():readSetting("pending").DOCHASH ~= nil, "and a 403")
respond(400, { error = "Not a document id" })
p:flush()
check(next(state():readSetting("pending")) == nil, "OPDShelf's own 400 drops it")
p:onCloseDocument()

-- 15. A file replaced by another one gets its own id
state():saveSetting("live", false)
doc_settings:saveSetting("opdshelf_sync", { synced = { timestamp = 1, device_id = "x", progress = "p" } })
local util_lib = require("util")
util_lib.partialMD5 = function() return "OTHERHASH" end
page_first = 10
p = open()
check(p.document == "OTHERHASH", "the id is worked out from the file, not taken from the sidecar")
check(doc_settings:readSetting("partial_md5_checksum") == "OTHERHASH" and doc_settings:readSetting("opdshelf_sync") == nil,
    "and what the plugin knew of the old book is forgotten")
p:onCloseDocument()
util_lib.partialMD5 = nil
doc_settings:saveSetting("partial_md5_checksum", "DOCHASH")

-- Every text of the plugin speaks Spanish
local missing = {}
-- 13. Finished books: KOReader's "Finished" status is synced as a mark
state():saveSetting("live", false)
doc_settings:delSetting("opdshelf_sync")
doc_settings:delSetting("summary")
state():saveSetting("pending", nil)
state():saveSetting("pending_marks", nil)
online = true
page_first = 100
p = open()
respond(200, { current = remote("KOBO-ID", 100) })
runScheduled()
local before_marks = #requests
doc_settings:saveSetting("summary", { status = "complete" })
respond(200, { marks = { DOCHASH = { state = "read", at = os.time() } }, server_time = os.time() })
p:onCloseDocument()
local mark_put
for i = before_marks + 1, #requests do if requests[i].url:match("/api/marks$") then mark_put = requests[i] end end
check(mark_put and mark_put.method == "PUT" and mark_put.body.documents[1] == "DOCHASH" and mark_put.body.state == "read"
    and mark_put.body.at and mark_put.body.sent_at, "marking the book Finished in KOReader sends a read mark")
check(next(state():readSetting("pending_marks")) == nil, "and the queue is empty once the server took it")

-- Not finished any more, read to 25 %: the mark is cleared (no state), not set to unread
p = open()
respond(200, { current = remote("KOBO-ID", 100), mark = { state = "read", at = os.time() - 5 } })
runScheduled()
doc_settings:saveSetting("summary", { status = "reading" })
respond(200, { marks = {}, server_time = os.time() })
p:onCloseDocument()
mark_put = nil
for i = #requests, 1, -1 do if requests[i].url:match("/api/marks$") then mark_put = requests[i] break end end
check(mark_put and mark_put.body.state == nil and mark_put.body.documents[1] == "DOCHASH", "no longer Finished below the threshold clears the mark")

-- A 404 from a proxy (the server is restarting) keeps the mark in the queue
doc_settings:delSetting("opdshelf_sync")
doc_settings:saveSetting("summary", { status = "complete" })
p = open()
respond(200, { current = remote("KOBO-ID", 100) })
runScheduled()
respond(404, "<html>Not found</html>")
p:onCloseDocument()
check(state():readSetting("pending_marks").DOCHASH and state():readSetting("pending_marks").DOCHASH.state == "read", "a 404 in HTML keeps the mark to send later")
respond(200, { marks = { DOCHASH = { state = "read", at = os.time() } }, server_time = os.time() })
p:flush()
check(next(state():readSetting("pending_marks")) == nil, "and it is sent when the network is back")

-- The server's marks are applied to the open book
doc_settings:delSetting("opdshelf_sync")
doc_settings:saveSetting("summary", { status = "reading" })
state():saveSetting("pending_marks", nil)
p = open()
respond(200, { current = remote("KOBO-ID", 100), mark = { state = "read", at = os.time() } })
runScheduled()
check(doc_settings:readSetting("summary").status == "complete", "a book marked as read elsewhere is Finished here")
p:onStreamEvent("marks", cjson.encode({ document = "DOCHASH", mark = { state = "unread", at = os.time() + 10 } }))
check(doc_settings:readSetting("summary").status == "reading", "and no longer when it is marked as unread")
p:onStreamEvent("marks", cjson.encode({ document = "DOCHASH", mark = { state = "read", at = os.time() + 20 } }))
check(doc_settings:readSetting("summary").status == "complete", "marked as read again")
p:onStreamEvent("marks", cjson.encode({ document = "DOCHASH", cleared_at = os.time() + 30 }))
check(doc_settings:readSetting("summary").status == "reading", "a cleared mark also unfinishes it")
p:onStreamEvent("marks", cjson.encode({ document = "DOCHASH", mark = { state = "read", at = os.time() + 5 } }))
check(doc_settings:readSetting("summary").status == "reading", "an older mark changes nothing")
p:onCloseDocument()

local I18n = dofile(plugin_dir .. "/opdshelf_i18n.lua")
for _i, name in ipairs({ "main", "opdshelf_browser", "opdshelf_home", "opdshelf_menu", "opdshelf_notes", "opdshelf_update", "opdshelf_sync", "opdshelf_client", "opdshelf_stream" }) do
    local f = io.open(plugin_dir .. "/" .. name .. ".lua")
    if f then
        local source = f:read("*a")
        f:close()
        -- Each _("...") with a whole literal inside, escapes read as Lua reads them
        local at = 1
        while true do
            local s, e = source:find('[^%w_]_%("', at)
            if not s then break end
            local i, closed = e + 1, nil
            while i <= #source do
                local c = source:sub(i, i)
                if c == "\\" then i = i + 2
                elseif c == '"' then closed = i break
                else i = i + 1 end
            end
            if closed and source:sub(closed + 1, closed + 1) == ")" then
                local literal = source:sub(e + 1, closed - 1)
                local text = loadstring('return "' .. literal .. '"')()
                if not I18n.es[text] then missing[#missing + 1] = name .. ": " .. literal end
            end
            at = (closed or e) + 1
        end
    end
end
check(#missing == 0, "every text has its Spanish version" .. (#missing > 0 and (":\n  " .. table.concat(missing, "\n  ")) or ""))

print("all good")

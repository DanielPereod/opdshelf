-- Builds every screen of the library browser against stand-ins for KOReader's widgets,
-- to catch mistakes in the code that lays them out.
-- Run from the repository root: luajit koreader/test/browser_test.lua koreader/opdshelf.koplugin
package.path = arg[1] .. "/?.lua;" .. package.path

local shown, closed, requests, ticks = {}, {}, {}, {}
local broadcasts, windows = {}, {}
local files = {}

-- The screen: changed by the tests to lay the home screen out on other devices
local screen = { w = 600, h = 800, scale = 1 }
local function scaled(n) return math.floor(n * screen.scale) end

-- Any widget: remembers what it was given and has a size, worked out roughly as KOReader does
local Widget = {}
Widget.__index = Widget
function Widget:extend(o) o = o or {} o.__index = o return setmetatable(o, { __index = self }) end
function Widget:new(o)
    o = setmetatable(o or {}, self)
    o.key_events = rawget(o, "key_events") or {}
    o.ges_events = rawget(o, "ges_events") or {}
    if o.init then o:init() end
    return o
end
local function textSize(self)
    local face = rawget(self, "face")
    local line = face and math.floor(face.size * 1.3) or 40
    local text = rawget(self, "text") or ""
    local lines = 1
    for _ in text:gmatch("\n") do lines = lines + 1 end
    local w = math.floor(#text * (face and face.size or 20) * 0.5)
    return w, line * lines
end
function Widget:getSize()
    local dimen = rawget(self, "dimen")
    local width, height = rawget(self, "width"), rawget(self, "height")
    local kind = self.kind
    if kind == "verticalgroup" or kind == "horizontalgroup" then
        local w, h = 0, 0
        for _, child in ipairs(self) do
            local size = child:getSize()
            if kind == "verticalgroup" then w, h = math.max(w, size.w), h + size.h else w, h = w + size.w, math.max(h, size.h) end
        end
        return { w = w, h = h }
    elseif kind == "verticalspan" then return { w = 0, h = width or 0 }
    elseif kind == "horizontalspan" then return { w = width or 0, h = 0 }
    elseif kind == "textwidget" then
        local w, h = textSize(self)
        local max = rawget(self, "max_width")
        return { w = max and math.min(w, max) or w, h = h }
    elseif kind == "textboxwidget" then
        -- Wrapped at its width, as a text box does (roughly: by letters, not words)
        local face = rawget(self, "face")
        local line = face and math.floor(face.size * 1.3) or 40
        local per_letter = (face and face.size or 20) * 0.5
        local lines = 0
        for part in ((rawget(self, "text") or "") .. "\n"):gmatch("([^\n]*)\n") do
            local n = 0
            for _c in part:gmatch("[%z\1-\127\194-\244][\128-\191]*") do n = n + 1 end
            lines = lines + math.max(1, math.ceil(n * per_letter / (width or 100)))
        end
        return { w = width or 100, h = height or line * lines }
    elseif kind == "button" then
        return { w = width or (rawget(self, "text") and math.floor(#self.text * 8 * screen.scale)) or scaled(40), h = scaled(40) }
    elseif kind == "framecontainer" and not (width and height) and self[1] then
        local size = self[1]:getSize()
        local pad = (rawget(self, "padding") or 5) + (rawget(self, "bordersize") or 0) + (rawget(self, "margin") or 0)
        return { w = width or size.w + 2 * pad, h = height or size.h + 2 * pad }
    elseif kind == "titlebar" then return { w = width or 100, h = scaled(60) }
    end
    return { w = width or (dimen and dimen.w) or 100, h = height or (dimen and dimen.h) or 40 }
end
function Widget:getHeight() return self:getSize().h end
function Widget:free() end
function Widget:paintTo() self.painted = (self.painted or 0) + 1 end
function Widget:resetLayout() end
function Widget:disable() self.disabled = true end
function Widget:getInputText() return self.input_text or "hobbit" end
function Widget:onShowKeyboard() end
function Widget:setText(text) self.text = text end

local widgets = {
    "ui/widget/container/bottomcontainer", "ui/widget/button", "ui/widget/buttondialog", "ui/widget/buttontable",
    "ui/widget/container/centercontainer", "ui/widget/confirmbox", "ui/widget/container/framecontainer",
    "ui/widget/horizontalgroup", "ui/widget/horizontalspan", "ui/widget/imagewidget", "ui/widget/infomessage",
    "ui/widget/container/inputcontainer", "ui/widget/inputdialog", "ui/widget/container/leftcontainer",
    "ui/widget/linewidget", "ui/widget/overlapgroup", "ui/widget/scrolltextwidget", "ui/widget/textboxwidget",
    "ui/widget/textwidget", "ui/widget/titlebar", "ui/widget/verticalgroup", "ui/widget/verticalspan",
    "ui/widget/pathchooser", "ui/widget/progresswidget", "ui/widget/container/rightcontainer", "ui/widget/iconwidget",
}
for _, name in ipairs(widgets) do
    package.preload[name] = function() return Widget:extend{ kind = name:match("([^/]+)$") } end
end
package.preload["ui/event"] = function() return { new = function(_, name, ...) return { name = name, args = { ... } } end } end
package.preload["ffi/blitbuffer"] = function() return { COLOR_WHITE = 0, COLOR_LIGHT_GRAY = 1, COLOR_GRAY_5 = 2, COLOR_BLACK = 3, COLOR_DARK_GRAY = 4 } end
local global = {}
_G.G_reader_settings = {
    readSetting = function(_, k) return global[k] end,
    saveSetting = function(_, k, v) global[k] = v end,
    isFalse = function(_, k) return global[k] == false end,
    isTrue = function(_, k) return global[k] == true end,
}
package.preload["datastorage"] = function() return { getDataDir = function() return "/tmp/opdshelf-test" end } end
package.preload["device"] = function() return {
    hasKeys = function() return true end, isTouchDevice = function() return true end,
    hasBattery = function() return true end,
    getPowerDevice = function() return { getCapacity = function() return 87 end, isCharging = function() return false end } end,
    input = { group = { Back = "Back", PgFwd = "PgFwd", PgBack = "PgBack" } },
    screen = { getWidth = function() return screen.w end, getHeight = function() return screen.h end, scaleBySize = function(_, n) return scaled(n) end },
} end
package.preload["ui/font"] = function() return { getFace = function(_, name, size) return { name = name, size = scaled(size or 20) } end } end
package.preload["ui/geometry"] = function()
    local Geom = {}
    Geom.__index = Geom
    function Geom:new(o) return setmetatable(o or {}, Geom) end
    function Geom:copy() local c = {} for k, v in pairs(self) do c[k] = v end return setmetatable(c, Geom) end
    return Geom
end
package.preload["ui/gesturerange"] = function() return { new = function(_, o) return o end } end
package.preload["ui/network/manager"] = function() return {
    isOnline = function() return true end, isWifiOn = function() return true end, runWhenOnline = function(_, f) f() end,
} end
-- Sizes follow the screen's scale, as KOReader's do
package.preload["ui/size"] = function()
    local function sizes(t) return setmetatable({}, { __index = function(_, k) return t[k] and math.max(1, scaled(t[k])) end }) end
    return {
        border = sizes{ thin = 1, button = 1 }, padding = sizes{ small = 2, default = 5, large = 10 },
        line = sizes{ thin = 1 }, radius = sizes{ window = 7 },
    }
end
package.preload["ui/uimanager"] = function() return {
    show = function(_, w, refresh) if type(w) == "table" then w.shown_refresh = refresh end shown[#shown + 1] = w end,
    close = function(_, w) closed[#closed + 1] = w end,
    -- What is due soon runs at once; the clock's minute ticks are kept to run by hand
    scheduleIn = function(_, s, f) if s < 1 then f() else ticks[#ticks + 1] = f end end,
    unschedule = function(_, f) for i = #ticks, 1, -1 do if ticks[i] == f then table.remove(ticks, i) end end end,
    nextTick = function(_, f) f() end,
    setDirty = function() end,
    -- Every window hears it: the ones the tests keep in `windows`
    broadcastEvent = function(_, event)
        broadcasts[#broadcasts + 1] = event
        for _, w in ipairs(windows) do
            local handler = w["on" .. event.name]
            if handler then handler(w, table.unpack and table.unpack(event.args) or unpack(event.args)) end
        end
    end,
    forceRePaint = function() end,
} end
package.preload["libs/libkoreader-lfs"] = function() return { attributes = function(path, what)
    local real = io.open(path, "rb")
    if real then real:close() end
    if files[path] or real then return what and "file" or { mode = "file" } end
end } end
package.preload["logger"] = function() return { warn = function() end, dbg = function() end } end
package.preload["ffi/sha2"] = function() return { md5 = function(s) return (s:gsub("[^%w]", "")) end } end
package.preload["util"] = function() return {
    makePath = function(dir) if dir:match("^/tmp/") then os.execute("mkdir -p '" .. dir .. "'") end end,
    trim = function(s) return (s:gsub("^%s+", ""):gsub("%s+$", "")) end,
    urlEncode = function(s) return (s:gsub(" ", "%%20")) end,
    htmlToPlainTextIfHtml = function(s) return s end,
} end
package.preload["ffi/util"] = function() return { template = function(s, ...)
    local args = { ... }
    return (s:gsub("%%(%d)", function(i) return tostring(args[tonumber(i)]) end))
end } end
package.preload["gettext"] = function() return function(s) return s end end
-- What KOReader keeps of each book; `doc_overrides[path]` changes some of it
local doc_overrides = {}
package.preload["docsettings"] = function() return {
    hasSidecarFile = function() return true end,
    open = function(_, path) return { readSetting = function(_, k)
        local over = doc_overrides[path] or {}
        if over[k] ~= nil then return over[k] end
        return ({
            percent_finished = 0.42,
            doc_props = { title = "Title of " .. path:match("[^/]+$"), authors = "Author 1\nAuthor 2" },
            partial_md5_checksum = "HASH-" .. path,
            summary = {},
        })[k]
    end } end,
} end
local history = {}
package.preload["readhistory"] = function() return { hist = history } end
package.preload["apps/reader/readerui"] = function() return { showReader = function(_, path) shown[#shown + 1] = { opened = path } end } end
package.preload["httpasync"] = function() return nil end
local trapper = { cancel = false, used = 0 }
package.preload["ui/trapper"] = function() return {
    wrap = function(_, f) f() end,
    -- The download runs here; a cancelled one never gets to run
    dismissableRunInSubprocess = function(_, task, widget)
        trapper.used = trapper.used + 1
        trapper.widget = widget
        if trapper.cancel then return false end
        return true, task()
    end,
} end

local function check(cond, msg) if not cond then error("FAILED: " .. msg, 2) end print("ok - " .. msg) end

local function entry(i)
    return string.format([[<entry><title>Book %d</title><id>urn:opdshelf:book:b%d.epub</id><author><name>Author %d</name></author>
<calibre:series>Saga</calibre:series><calibre:series_index>%d</calibre:series_index><summary>About book %d</summary>
<link rel="http://opds-spec.org/image/thumbnail" href="/thumb/b%d.epub" type="image/jpeg"/>
<link rel="http://opds-spec.org/image" href="/cover/b%d.epub" type="image/jpeg"/>
<link rel="http://opds-spec.org/acquisition" href="/download/b%d.epub" type="application/epub+zip" length="2048"/></entry>]], i, i, i, i, i, i, i, i)
end
local function feedXml(from, to, next_page)
    local parts = { '<feed xmlns="http://www.w3.org/2005/Atom"><title>Books</title>' }
    if next_page then parts[#parts + 1] = '<link rel="next" href="/opds/books?page=' .. next_page .. '"/>' end
    for i = from, to do parts[#parts + 1] = entry(i) end
    parts[#parts + 1] = "</feed>"
    return table.concat(parts)
end
local root = [[<feed xmlns="http://www.w3.org/2005/Atom"><title>OPDShelf</title>
<link rel="search" href="/opds/search?q={searchTerms}" type="application/atom+xml"/>
<entry><title>All books</title><content>40 books</content><link rel="subsection" href="/opds/books"/></entry>
<entry><title>Authors</title><content>3 authors</content><link rel="subsection" href="/opds/authors"/></entry></feed>]]

local client = {
    server = "https://books.example",
    fetch = function(_, url)
        requests[#requests + 1] = url
        if url == "https://books.example/" then return 200, root end
        if url:match("/opds/books%?page=2$") then return 200, feedXml(21, 40) end
        if url:match("/opds/books$") then return 200, feedXml(1, 20, 2) end
        if url:match("/opds/search") then return 200, feedXml(1, 1) end
        if url:match("/opds/reading$") then return 200, feedXml(5, 5) end
        return 404, ""
    end,
    raw = function(_, _m, url)
        requests[#requests + 1] = url
        return 200, string.rep("x", 200)
    end,
    headers = function() return {} end,
    getProgress = function(_, document)
        requests[#requests + 1] = "progress " .. document
        return 200, { current = { device = "Phone", device_id = "PHONE", percentage = 0.6, timestamp = os.time() - 120 } }
    end,
    getToday = function(_, date) requests[#requests + 1] = "today " .. tostring(date) return 200, { seconds = 1530, sessions = 2, streak = 4 } end,
    -- Where every book is: the one to continue further on the phone, one started
    -- there only, one finished, one marked as not finished afterwards and one
    -- read here but marked as finished on another device
    getLibraryProgress = function()
        requests[#requests + 1] = "library progress"
        local now = os.time()
        local function at(device, pct, age) return { device = device, device_id = device, percentage = pct, timestamp = now - age, progress = "/x" } end
        return 200, {
            server_time = now,
            books = {
                { document = "HASH-/books/b1.epub", path = "b1.epub", title = "Book 1", current = at("PHONE", 0.6, 120) },
                { document = "D5", path = "b5.epub", title = "Book 5", current = at("PHONE", 0.3, 3600) },
                { document = "D6", path = "b6.epub", title = "Book 6", current = at("PHONE", 0.995, 60) },
                { document = "D7", path = "b7.epub", title = "Book 7", current = at("PHONE", 0.5, 9000) },
                -- No longer in the library
                { document = "D9", current = at("PHONE", 0.5, 30) },
            },
            marks = {
                { document = "D7", path = "b7.epub", state = "unread", at = now - 100 },
                { document = "HASH-/books/done-elsewhere.epub", state = "read", at = now - 100 },
            },
        }
    end,
    download = function(_, url, path)
        requests[#requests + 1] = url
        files[path] = true
        return true
    end,
}
local settings = {}
local plugin = {
    client = client,
    state = { readSetting = function(_, k) return settings[k] end, saveSetting = function(_, k, v) settings[k] = v end, flush = function() end },
    downloadDir = function() return "/books" end,
    chooseDownloadDir = function() end,
    ready = function() return true end,
    device_id = "KOBO",
    noteServerTime = function() end,
    -- Sends the positions read here and not sent yet
    flush = function(self) self.flushed = (self.flushed or 0) + 1 return false end,
    serverNow = function() return os.time() end,
    launcherEnabled = function(self) return self.launcher end,
    showStatus = function(self) self.status_shown = true end,
    editAccount = function(self, done) self.account_edited = true if done then done() end end,
    setLauncher = function(self, on) self.launcher = on end,
}
-- The settings, as main.lua gives them to Tools › OPDShelf and the home screen
function plugin:settingsItems(changed)
    self.settings_changed = changed
    return {
        { text = "Show the home screen at start", checked_func = function() return self:launcherEnabled() end,
          callback = function() self:setLauncher(not self:launcherEnabled()) end },
        { text = "When another device is behind", sub_item_table = {
            { text = "Ask", radio = true, checked_func = function() return self.behind ~= "jump" end, callback = function() self.behind = "ask" end },
            { text = "Go there", radio = true, checked_func = function() return self.behind == "jump" end, callback = function() self.behind = "jump" end },
        } },
    }
end
function plugin:showLibrary(search, url, title) self.library_opened = { search = search, url = url, title = title } end

os.execute("rm -rf /tmp/opdshelf-test")
local Browser = require("opdshelf_browser")
local b = Browser:new{ plugin = plugin }
b:start()
check(b:currentLevel().title == "OPDShelf library" and #b:currentLevel().entries == 2, "opens the catalog root")
b:select(b:currentLevel().entries[1])
local level = b:currentLevel()
check(#b.stack == 2 and #level.entries == 20 and level.next, "opens a section with a next page")
local covers = 0
for _, url in ipairs(requests) do if url:match("/thumb/") then covers = covers + 1 end end
check(covers > 0, "downloads the covers shown")
local per_page = b:layout(level).per_page
for _ = 1, 20 do b:onNextPage() end
check(#level.entries == 40 and not level.next, "loads the next page of the feed when reaching the end")
check(level.page == math.ceil(40 / per_page), "and stops at the last page")
b:onPrevPage()
check(level.page == math.ceil(40 / per_page) - 1, "goes back a page")

-- Grid
settings.library_view = nil
b.view = "grid"
level.page = 1
b:refresh()
check(b:layout(level).grid and b:layout(level).per_page == 9, "shows covers in a 3×3 grid")
b:showMenu()
local menu = shown[#shown]
check(menu.buttons and #menu.buttons >= 4, "has a menu")
menu.buttons[3][1].callback()
check(b.view == "list" and settings.library_view == "list", "switches back to the list and remembers it")

-- Details and download
b:select(level.entries[1])
local details = shown[#shown]
check(details[1] ~= nil, "shows a book's details")
check(details.shown_refresh == "flashui", "and refreshes the screen for them (e-ink shows nothing otherwise)")
local function findButtons(w, found)
    found = found or {}
    if type(w) ~= "table" then return found end
    if w.buttons and w.kind == "buttontable" then found[#found + 1] = w end
    for _, child in ipairs(w) do findButtons(child, found) end
    for _, key in ipairs({ "content" }) do findButtons(w[key], found) end
    return found
end
local tables = findButtons(details[1])
check(#tables == 1 and tables[1].buttons[1][2].text == "Download and read", "with download buttons")
tables[1].buttons[1][2].callback()
check(files["/books/b1.epub"] and shown[#shown].opened == "/books/b1.epub", "downloads and opens it")
check(b.closed, "closing the library")

-- Downloaded books are marked
local b2 = Browser:new{ plugin = plugin }
b2:start("https://books.example/opds/books", "Books")
-- Book 1 is here at 42 %, read at a time not known; the phone took it to 60 % two minutes ago
local feed_entries = b2:currentLevel().entries
check(b2:statusLine(feed_entries[1]):match("· 60 %% read$"), "a book read further on another device shows that, without a check mark")
doc_overrides["/books/b1.epub"] = { opdshelf_sync = { read_at = os.time() } }
check(b2:statusLine(feed_entries[1]):match("· 42 %% read$") and b2:readBar(feed_entries[1]) == 0.42,
    "and one read here since shows how far it was read here, with its bar")
doc_overrides["/books/b1.epub"] = { summary = { status = "complete" }, percent_finished = 1 }
check(b2:statusLine(feed_entries[1]):match("✓ Finished$") and not b2:readBar(feed_entries[1]), "the check is for finished books")
doc_overrides["/books/b1.epub"] = nil
-- Book 5 is not on this device: started on the phone
check(b2:statusLine(feed_entries[5]):match("· 30 %% read$") and b2:readBar(feed_entries[5]) == 0.3,
    "every started book shows how far it was read, wherever it was read")
check(b2:statusLine(feed_entries[6]):match("✓ Finished$") and not b2:statusLine(feed_entries[7]):match("read"),
    "finished elsewhere is finished, and marked as not finished afterwards is not started")
doc_overrides["/books/b2.epub"] = { percent_finished = false }
files["/books/b2.epub"] = true
check(b2:statusLine(feed_entries[2]):match("· Downloaded$"), "one only downloaded says so in words: ✓ is only for finished books")
files["/books/b2.epub"] = nil
doc_overrides["/books/b2.epub"] = nil
check(Browser.mergeProgress({ percent = 0.2, read_at = 100 }, { percent = 0.5, time = 50 }).percent == 0.2
    and Browser.mergeProgress({ percent = 0.2, read_at = 100 }, { percent = 0.5, time = 150 }).percent == 0.5
    and Browser.mergeProgress(nil, { percent = 0.1, finished = true }).finished and Browser.mergeProgress(nil, nil) == nil,
    "the newest position wins, and finished anywhere is finished")
check(Browser.formatSize(1677722) == "1.6 MB" and Browser.languageName("es") == "Spanish" and Browser.languageName("xx") == "xx",
    "sizes and the language's name")
global.language = "es"
check(Browser.formatSize(1677722) == "1,6 MB" and Browser.languageName("es-ES") == "Español" and Browser.languageName("eng") == "Inglés",
    "in Spanish with a decimal comma and the language's name in Spanish")
global.language = nil
b2:quickDownload(b2:currentLevel().entries[2])
check(shown[#shown].ok_text == "Read now", "a long press downloads and offers to read")
local hold = b2:bookActions(b2:currentLevel().entries[3])
check(hold.title == b2:currentLevel().entries[3].title and hold.buttons[1][1].text == "Download and read"
    and hold.buttons[2][1].text == "Download only" and hold.buttons[3][1].text == "Book details",
    "holding a book that is not here offers to read, download or see it")
local held = b2:bookActions(b2:currentLevel().entries[2])
check(held.buttons[1][1].text == "Read" and #held.buttons == 2, "and one already downloaded offers to read it")
local before_details = #shown
held.buttons[2][1].callback()
check(#shown == before_details + 1 and shown[#shown][1] ~= nil, "or to see its details")
check(trapper.used == 2 and trapper.widget.text:match("Tap to cancel"), "downloads away from the tap, in a subprocess that can be cancelled")
trapper.cancel = true
local before = #shown
b2:quickDownload(b2:currentLevel().entries[3])
check(not files["/books/b3.epub"] and shown[#shown].text == "Download cancelled." and #shown == before + 2,
    "a cancelled download says so and offers nothing to read")
trapper.cancel = false

-- Search
b2:search()
local dialog = shown[#shown]
dialog.buttons[1][2].callback()
local searched = false
for _, url in ipairs(requests) do searched = searched or url == "https://books.example/opds/search?q=hobbit" end
check(searched, "searches the catalog")
check(b2:currentLevel().title == "Search: hobbit" and #b2:currentLevel().entries == 1, "shows the results")
b2:goBack()
check(b2:currentLevel().title == "Books", "goes back up")
b2:onClose()

-- The library opens again where it was left
local b3 = Browser:new{ plugin = plugin }
plugin.browser = b3
b3:start()
b3:select(b3:currentLevel().entries[1])
b3:onClose()
check(plugin.browser == nil and plugin.library_session and #plugin.library_session.stack == 2, "remembers where the library was")
local b4 = Browser:new{ plugin = plugin }
b4:resume(plugin.library_session)
check(b4:currentLevel().title == "All books" and not b4.closed, "and opens there again")
-- Turned: Android tells every window the new size
screen.w, screen.h = 800, 600
b4:onSetDimensions({ w = 800, h = 600 })
b4:onScreenResize({ w = 800, h = 600 })
check(b4.dimen.w == 800 and b4[1].width == 800 and b4.ges_events.Swipe[1].range.w == 800 and b4:layout(b4:currentLevel()).w == 800,
    "the library takes the new size of a turned screen")
screen.w, screen.h = 600, 800
b4:paintTo()
check(b4.dimen.w == 600 and b4[1].width == 600 and b4.painted == 1, "and finds it out when painted, whatever turned it")
b4:onClose()
check(settings.covers and settings.covers["/books/b1.epub"], "downloads keep their cover for the home screen")


-- Home screen
local function texts(w, found)
    found = found or {}
    if type(w) ~= "table" then return found end
    if type(rawget(w, "text")) == "string" then found[#found + 1] = w.text end
    if type(rawget(w, "subtitle")) == "string" then found[#found + 1] = w.subtitle end
    for _, child in ipairs(w) do texts(child, found) end
    return found
end
local function has(list, pattern)
    for _, t in ipairs(list) do if t:match(pattern) then return t end end
end
local function hasText(list, text)
    for _, t in ipairs(list) do if t == text then return true end end
end
local Home = require("opdshelf_home")
local empty = Home:new{ plugin = plugin }
empty:refresh()
check(has(texts(empty), "^Nothing read yet$") and has(texts(empty), "^Open the library$"), "home without history invites to the library")
check(has(texts(empty), "^Books you start, here or on your other devices, will show here%.$"), "and says where started books will be")
check(hasText(texts(empty), "–"), "tiles not known yet show a dash")
empty:onClose()

-- Started books: the history here and the server's positions, merged
local now = os.time()
local merged = Home.mergeStarted({
    { path = "/b/continue.epub", title = "Continue", percent = 0.5, time = now - 10, document = "C" },
    { path = "/b/here.epub", title = "Here", percent = 0.2, time = now - 500, document = "H" },
    { path = "/b/guess.epub", title = "Guess", percent = 0.1, time = now - 5, time_guess = true, document = "G" },
    { path = "/b/done.epub", title = "Done", percent = 1, time = now - 1, finished = true, document = "F" },
    { path = "/b/new.epub", title = "Not started", percent = 0, time = now - 2 },
}, {
    { document = "R", title = "Remote", percent = 0.3, time = now - 100, entry = { title = "Remote", acquisitions = {} } },
    { document = "G", title = "Guess", percent = 0.15, time = now - 4000 },
    { document = "H", title = "Here", percent = 0.9, time = now - 9000 },
    { document = "X", title = "Finished elsewhere", percent = 0.995, finished = true, time = now, entry = {} },
    { document = "Y", title = "Downloaded", percent = 0.4, time = now - 50, entry = { file = "/b/dl.epub" } },
    { document = "Z", title = "Gone from the library", percent = 0.4, time = now - 20 },
}, { path = "/b/continue.epub", document = "C" }, function(entry) return entry.file end)
local order = {}
for i, r in ipairs(merged) do order[i] = r.title end
check(table.concat(order, "|") == "Downloaded|Remote|Here|Guess", "started books: newest first, finished, unstarted and the book to continue left out")
check(merged[1].on_device and merged[1].path == "/b/dl.epub" and not merged[2].on_device, "a book of the library already downloaded opens here, the others are downloaded first")
check(merged[3].percent == 0.2 and merged[4].percent == 0.15 and merged[4].time == now - 4000,
    "the newest position wins, and a time only guessed from KOReader's history gives way to the server's")
local remote = Home.remoteStarted(select(2, plugin.client.getLibraryProgress()), nil, 100, "https://books.example")
local by_doc = {}
for _, r in ipairs(remote) do by_doc[r.document] = r end
check(by_doc.D5 and math.abs(by_doc.D5.time - (os.time() - 3700)) <= 1 and by_doc.D5.entry.acquisitions[1].href == "https://books.example/download/b5.epub",
    "the server's books come on this device's clock, with a way to download them")
check(by_doc.D6.finished and not by_doc.D7 and by_doc["HASH-/books/done-elsewhere.epub"].finished, "read to the end or marked as finished counts as finished, marked as not finished afterwards as not started")
check(Home.libraryPath({ id = "urn:opdshelf:book:Tolkien/El%20hobbit.epub" }) == "Tolkien/El hobbit.epub", "library paths from OPDS ids")

-- The tiles
local day = os.date("%Y-%m-%d")
local tiles = Home.statTexts({ date = day, today_seconds = 33, streak = 1, count = 40 }, day)
check(tiles.today == "0 min" and tiles.streak == "1 day" and tiles.library == "40", "tiles: whole minutes today, the streak and the library")
check(Home.statTexts({ date = day, today_seconds = 3900 }, day).today == "1 h 5 min" and Home.statTexts({ date = day, today_seconds = 7200 }, day).today == "2 h",
    "an hour or more in hours")
tiles = Home.statTexts({ date = "2001-01-01", today_seconds = 600, streak = 3 }, day)
check(tiles.today == nil and tiles.streak == "3 days" and tiles.library == nil, "minutes from another day are not today's, and what is not known stays empty")

-- The card's status line: the pages left go first when it does not fit
local status_book = { title = "Status", authors = {}, percent = 0.16, time = os.time() - 7200, pages = 300, path = "/books/status.epub" }
check(Home.progressText(nil, status_book, true) == "16 % · 2 h ago · 252 pages left" and Home.progressText(nil, status_book, true, false) == "16 % · 2 h ago",
    "the status line with or without the pages left")
local function statusOf(card)
    for _, t in ipairs(texts(card)) do if t:match("^16 %%") then return t end end
end
local card_home = Home:new{ plugin = plugin }
card_home.history = { status_book }
check(statusOf((card_home:continueCard(1500, 800))) == "16 % · 2 h ago · 252 pages left", "a wide card says it all")
check(statusOf((card_home:continueCard(450, 800))) == "16 % · 2 h ago", "a narrower one leaves out the pages left rather than cutting the line")
check(statusOf((card_home:continueCard(320, 800))) == "16 % · 2 h", "and a narrow one says when in a few letters")
card_home:onClose()

files["/books/other.epub"] = true
files["/books/done.epub"] = true
files["/books/done-elsewhere.epub"] = true
doc_overrides["/books/other.epub"] = { opdshelf_sync = { read_at = now - 7200 } }
doc_overrides["/books/done.epub"] = { summary = { status = "complete" }, percent_finished = 1 }
history[1] = { file = "/books/b1.epub", time = now - 60 }
-- Opened a minute ago but read two hours ago
history[2] = { file = "/books/other.epub", time = now - 60 }
history[3] = { file = "/books/gone.epub", time = now - 9000 }
history[4] = { file = "/books/done.epub", time = now - 9500 }
history[5] = { file = "/books/done-elsewhere.epub", time = now - 9600 }
requests = {}
local home = Home:new{ plugin = plugin }
plugin.home = home
home:show()
local seen = texts(home)
if os.getenv("DEBUG") then for _, t in ipairs(seen) do print("  text:", t) end end
check(#home.history == 4, "looks at the books read on this device that are still there")
local asked_today
for i, r in ipairs(requests) do if r:match("^today ") then asked_today = i end end
check(asked_today and requests[asked_today] == "today " .. os.date("%Y-%m-%d") and plugin.flushed == 1,
    "today's minutes are asked for this device's day, once what was read here is sent")
check(hasText(seen, "25 min") and hasText(seen, "4 days") and hasText(seen, "40") and hasText(seen, "Today") and hasText(seen, "Streak"),
    "tiles with the minutes read today, the streak and the size of the library")
check(#home.recent == 12 and home.recent[1].title == "Book 1", "shows the newest books of the library")
check(has(seen, "^Title of b1%.epub$") and has(seen, "^42 %% · 1 min ago") and hasText(seen, "Continue ›"), "continue reading: the last book, how far, when and a button")
check(home.history[1].remote and has(seen, "^Further on PHONE: 60 %%"), "and where another device went further")
order = {}
for i, r in ipairs(home.started) do order[i] = r.title end
check(table.concat(order, "|") == "Book 5|Title of other.epub", "started: from the server and from here, newest first, without finished books")
check(hasText(seen, "Started") and hasText(seen, "30 % · 1 h") and hasText(seen, "42 % · 2 h"), "a shelf of them, with how far and when")
check(hasText(seen, "↓") and hasText(seen, "Downloaded") and not hasText(seen, "✓"),
    "books not on this device are marked, and downloaded ones say so in words: ✓ is only for finished books")
check(has(seen, "^Recently added to the library$"), "the newest books of the library")
check(has(seen, "^%d+:%d%d · %d+ %a+$") and has(seen, "^87 %%$") and hasText(seen, "OPDShelf"), "a header with the name, the time, date and battery")
check(not has(seen, "^On this device$") and not has(seen, "^Offline"), "without the old shelf, and connected")
for _, label in ipairs({ "Home", "Library", "Search", "Files", "More" }) do
    check(hasText(seen, label), "a tab for " .. label)
end
check(settings.home_started and #settings.home_started >= 2 and settings.home_stats.streak == 4 and #settings.home_recent == 12,
    "keeps the server's part for when there is no connection")
check(#ticks == 1, "the clock waits for the next minute")
local clock_before = home.clock_widget.text
home.clock_widget.text = "old"
table.remove(ticks)()
check(home.clock_widget.text == clock_before and #ticks == 1, "and then updates itself")

-- Tapping and holding started books
local before = #shown
home:openRecord(home.started[1])
check(#shown == before + 1 and shown[#shown][1] ~= nil and not home.closed, "a book not on this device opens its details, to download it")
local hold = home:recordActions(home.started[1])
check(hold.buttons[1][1].text == "Download and read", "holding it offers the library's menu")
check(hold.width and hold.width >= 400 and not hold.shrink_unneeded_width, "a hold menu wide enough for its title")
local long = Browser.holdDialog(string.rep("A very long title ", 20), {})
check(long.title:match("…$") and #long.title < 200, "and a long title cut with an ellipsis")
-- As wide as the dialog's title is, in its face: two lines at most, however long the words
local TextBoxWidget = require("ui/widget/textboxwidget")
local Font = require("ui/font")
for _, title in ipairs({ string.rep("Comprehensive ", 9), "El señor de los anillos: la comunidad del anillo, edición ilustrada completa", "Short" }) do
    local cut = Browser.holdDialog(title, {}).title
    local box = TextBoxWidget:new{ text = cut, face = Font:getFace("infofont"), width = long.width - 4 * 10 }
    local two = TextBoxWidget:new{ text = "Ag\nAg", face = Font:getFace("infofont"), width = long.width - 4 * 10 }
    check(box:getSize().h <= two:getSize().h and (cut == title or cut:match("…$")) and not cut:match(" …$"),
        "the hold menu's title takes two lines at most: " .. cut)
end
check(Browser.holdDialog("Short", {}).title == "Short", "and a short one stays whole")
hold = home:recordActions(home.started[2])
check(hold.buttons[1][1].text == "Read", "and a book on this device offers to read it")

-- All the started books
local refreshes = 0
local home_refresh = home.refresh
home.refresh = function(...) refreshes = refreshes + 1 return home_refresh(...) end
local list = home:showStarted()
check(#list.books == #home.started + 1 and list.books[1].path == "/books/b1.epub" and hasText(texts(list), "3 books"),
    "see all also lists the book of the Continue card, and counts it")
-- Turning the screen: Android sends SetDimensions and ScreenResize to every window
windows = { home, list }
screen.w, screen.h = 800, 600
local UIManager = require("ui/uimanager")
local Event = require("ui/event")
UIManager:broadcastEvent(Event:new("SetDimensions", { w = 800, h = 600 }))
UIManager:broadcastEvent(Event:new("ScreenResize", { w = 800, h = 600 }))
check(list.dimen.w == 800 and list[1].width == 800 and list.ges_events.Swipe[1].range.w == 800,
    "the full list takes the new size of a turned screen")
check(home.dimen.w == 800 and home[1].width == 800 and home.ges_events.Swipe[1].range.h == 600 and home.layout_info.left ~= nil
    and refreshes == 1, "and the home screen under it is laid out again, once, in landscape")
-- Turned with the home screen on top (a gesture, the gyro): the file browser under it turns the screen
local turned
package.loaded["apps/filemanager/filemanager"] = { instance = { onSetRotationMode = function(_, mode)
    turned = mode
    screen.w, screen.h = 600, 800
end } }
broadcasts = {}
check(home:onSetRotationMode(0) and turned == 0 and broadcasts[1].name == "ScreenResize", "turning from the home screen goes to the file browser")
check(home.dimen.w == 600 and home[1].width == 600 and not home.layout_info.left and list.dimen.w == 600 and refreshes == 2,
    "and every window is laid out again for it")
package.loaded["apps/filemanager/filemanager"] = nil
check(home:onSetRotationMode(1) == false, "without a file browser that turns it, KOReader goes on as it would")
windows = {}
home.refresh = home_refresh
check(shown[#shown] == list and has(texts(list), "^Book 5$") and has(texts(list), "on this device$") and has(texts(list), "in the library$"),
    "see all lists every started book, here or not")
list:onClose()

-- The menu: KOReader's screens, then the plugin's
local home_menu = home:showMenu()
check(home_menu.buttons[1][1].text == "KOReader" and home_menu.buttons[1][1].enabled == false
    and home_menu.buttons[2][1].text == "File browser" and home_menu.buttons[2][2].text == "History"
    and home_menu.buttons[3][1].text == "Favorites" and home_menu.buttons[3][2].text == "Collections"
    and home_menu.buttons[4][1].text == "File search" and home_menu.buttons[4][2].text == "KOReader menu",
    "the menu goes to KOReader's own screens")
check(home_menu.buttons[5][1].text == "OPDShelf" and home_menu.buttons[6][1].text == "Refresh"
    and home_menu.buttons[6][2].text == "Settings ›" and home_menu.buttons[7][1].text == "Status and updates",
    "and has the plugin's things apart")
home_menu.buttons[7][1].callback()
check(plugin.status_shown, "the status opens")
home_menu.buttons[6][2].callback()
local settings_menu = shown[#shown]
check(settings_menu.buttons[1][1].text == "☐ Show the home screen at start", "the settings show the plugin's own settings, ticked or not")
check(settings_menu.buttons[2][1].text == "When another device is behind ›", "and their submenus")
settings_menu.buttons[1][1].callback()
check(plugin.launcher == true, "the menu turns opening at start on")
check(plugin.settings_changed ~= nil, "and the home screen hears about changes to the account")
home:showSettings()
shown[#shown].buttons[2][1].callback()
local choices = shown[#shown]
check(choices.buttons[1][1].text == "● Ask" and choices.buttons[2][1].text == "○ Go there", "a submenu shows its choices")
choices.buttons[2][1].callback()
check(plugin.behind == "jump" and shown[#shown].buttons[2][1].text == "When another device is behind ›",
    "and picking one goes back to the settings")
home:openLibrary(false, home.recent_url, "Recently added")
check(plugin.library_opened.url == "https://books.example/opds/books", "see all opens the library there")
before = #shown
home.browser:showDetails(home.recent[2])
check(#shown == before + 1, "a book of the shelf shows its details")
home:openBook("/books/other.epub")
check(home.closed and plugin.home == nil and shown[#shown].opened == "/books/other.epub", "opening a book leaves the home screen")
check(#ticks == 0, "and stops its clock")

-- KOReader's own screens, through the file browser under the home screen
local calls = {}
local function module(name, method) return { [method] = function(self) calls[#calls + 1] = name .. ":" .. method assert(self) end } end
local fm = {
    history = module("history", "onShowHist"),
    collections = { onShowColl = function() calls[#calls + 1] = "favorites" end, onShowCollList = function() calls[#calls + 1] = "collections" end },
    filesearcher = module("filesearcher", "onShowFileSearch"),
    menu = module("menu", "onShowMenu"),
}
package.loaded["apps/filemanager/filemanager"] = { instance = fm, showFiles = function() calls[#calls + 1] = "showFiles" end }
local expected = {
    files = nil, history = "history:onShowHist", favorites = "favorites", collections = "collections",
    search = "filesearcher:onShowFileSearch", menu = "menu:onShowMenu",
}
local h
for _, id in ipairs({ "files", "history", "favorites", "collections", "search", "menu" }) do
    calls = {}
    h = Home:new{ plugin = plugin }
    local opened = h:openNative(id)
    check(opened and calls[1] == expected[id] and ((h.closed or false) == (id == "files")),
        "KOReader's " .. id .. (id == "files" and " opens, leaving the home screen" or " opens over the home screen, so closing it comes back"))
    h:onClose()
end
-- A screen over the home screen that goes to a folder of the file browser: the home screen leaves
fm.file_chooser = { changeToPath = function(self, path) calls[#calls + 1] = "folder " .. path assert(self == fm.file_chooser) end }
calls = {}
h = Home:new{ plugin = plugin }
h:openNative("history")
check(not h.closed and rawget(fm.file_chooser, "changeToPath") ~= nil, "History shows over the home screen")
fm.file_chooser:changeToPath("/books/saga")
check(h.closed and calls[#calls] == "folder /books/saga", "and its Show folder leaves it, to show the folder")
h = Home:new{ plugin = plugin }
h:openNative("search")
local wrapped = fm.file_chooser.changeToPath
h:onClose()
check(fm.file_chooser.changeToPath ~= wrapped and fm.file_chooser.changeToPath ~= nil, "closing the home screen gives the file browser its own way back")
fm.file_chooser = nil
fm.collections = nil
fm.menu = { onShowMenu = function() error("broken") end }
calls = {}
h = Home:new{ plugin = plugin }
check(h:openNative("collections") == false and h.closed and #calls == 0, "a screen this KOReader lacks leaves the file browser instead")
h = Home:new{ plugin = plugin }
check(h:openNative("menu") == false and h.closed, "and one that fails too, without an error")
package.loaded["apps/filemanager/filemanager"] = { showFiles = function() calls[#calls + 1] = "showFiles" end }
calls = {}
h = Home:new{ plugin = plugin }
check(h:openNative("history") == false and calls[1] == "showFiles", "without a file browser at all, it opens one")
h = Home:new{ plugin = plugin }
h:refresh()
check(h:onSwipe(nil, { direction = "south", pos = { y = 2 } }) and calls[#calls] == "showFiles" and h.closed,
    "a swipe down from the top goes to KOReader's menu")
package.loaded["apps/filemanager/filemanager"] = nil

-- Every screen: nothing overlaps the tabs, whatever the size
local function fits(name, w, hgt, scale)
    screen.w, screen.h, screen.scale = w, hgt, scale
    local s = Home:new{ plugin = plugin }
    s:refresh()
    local page = s[1][1]
    local body_h = page[1]:getSize().h
    local tabs_h = page[2][2]:getSize().h
    check(body_h <= hgt - tabs_h, string.format("%s: the page (%d) fits above the tabs (%d of %d)", name, body_h, tabs_h, hgt))
    local L = s.layout_info
    print(string.format("  %s: %d per row, %d row(s) of started, covers %dx%d, captions %s, recently added %s%s",
        name, L.count, L.rows, L.cover_w, L.cover_h, tostring(L.captions), tostring(L.recent),
        L.recent_beside and " (recently added under Started, the card in a column of its own)"
            or L.left and string.format(" (left: %d per row, covers %dx%d)", L.left.count, L.left.cover_w, L.left.cover_h) or ""))
    s:onClose()
    return L
end
-- Enough started books for two rows
local many = {}
for i = 1, 20 do many[i] = { document = "M" .. i, title = "Book " .. i, percent = 0.5, time = now - i, entry = { title = "Book " .. i, acquisitions = {} } } end
settings.home_started = many
plugin.ready = function() return false end
local tablet = fits("Xiaomi Pad 6, portrait", 1800, 2880, 2.5)
check(tablet.rows == 2 and tablet.captions and tablet.recent, "a tall screen gets a second row of started books")
local landscape = fits("Xiaomi Pad 6, landscape", 2880, 1800, 2.5)
check(landscape.right and landscape.recent and landscape.rows >= 1 and landscape.cover_h >= tablet.cover_h * 0.8,
    "landscape puts the card and the newest books beside Started, with covers as large")
-- Few started books: the newest books go under them, with their titles, beside a tall card
local few = {}
for i = 1, 3 do few[i] = many[i] end
settings.home_started = few
local beside = fits("Xiaomi Pad 6, landscape, 3 started", 2880, 1800, 2.5)
check(beside.recent_beside and beside.captions and beside.left.card_tall and beside.rows == 1,
    "landscape with few started books puts the newest books under them, with titles, and the card fills its column")
settings.home_started = many
check(not fits("Xiaomi Pad 6, landscape, 20 started", 2880, 1800, 2.5).recent_beside, "and with many, Started keeps its column")
local small = fits("6\" e-reader", 1072, 1448, 1.875)
check(small.rows == 1, "a small screen keeps one row")
fits("small, landscape", 1448, 1072, 1.875)
fits("small, large font", 1072, 1448, 2.6)
screen.w, screen.h, screen.scale = 600, 800, 1
plugin.ready = function() return true end

-- Without a connection it shows what it kept
local NetworkMgr = require("ui/network/manager")
NetworkMgr.isOnline = function() return false end
settings.home_started = nil
local offline_requests = #requests
local offline = Home:new{ plugin = plugin }
offline:show()
check(#offline.recent == 12 and has(texts(offline), "^Offline"), "offline it shows the kept shelf and says so")
check(#requests == offline_requests, "and asks nothing of the server")
-- The kept started books, from the last connection
settings.home_started = Home.remoteStarted(select(2, plugin.client.getLibraryProgress()), nil, 0, "https://books.example")
offline = Home:new{ plugin = plugin }
offline:show()
check(offline.started[1] and offline.started[1].title == "Book 5" and hasText(texts(offline), "25 min") and hasText(texts(offline), "4 days"),
    "and the started books of other devices and the last statistics")
print("all good")

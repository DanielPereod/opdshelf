-- Builds every screen of the library browser against stand-ins for KOReader's widgets,
-- to catch mistakes in the code that lays them out.
-- Run from the repository root: luajit koreader/test/browser_test.lua koreader/opdshelf.koplugin
package.path = arg[1] .. "/?.lua;" .. package.path

local shown, closed, requests, ticks = {}, {}, {}, {}
local files = {}

-- Any widget: remembers what it was given and has a size
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
function Widget:getSize()
    local dimen = rawget(self, "dimen")
    return { w = rawget(self, "width") or (dimen and dimen.w) or 100, h = rawget(self, "height") or (dimen and dimen.h) or 40 }
end
function Widget:getHeight() return self:getSize().h end
function Widget:free() end
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
package.preload["ffi/blitbuffer"] = function() return { COLOR_WHITE = 0, COLOR_LIGHT_GRAY = 1, COLOR_GRAY_5 = 2, COLOR_BLACK = 3 } end
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
    screen = { getWidth = function() return 600 end, getHeight = function() return 800 end, scaleBySize = function(_, n) return n end },
} end
package.preload["ui/font"] = function() return { getFace = function(_, name, size) return { name = name, size = size or 20 } end } end
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
package.preload["ui/size"] = function() return {
    border = { thin = 1, button = 1 }, padding = { small = 2, default = 5, large = 10 }, line = { thin = 1 }, radius = { window = 7 },
} end
package.preload["ui/uimanager"] = function() return {
    show = function(_, w, refresh) if type(w) == "table" then w.shown_refresh = refresh end shown[#shown + 1] = w end,
    close = function(_, w) closed[#closed + 1] = w end,
    -- What is due soon runs at once; the clock's minute ticks are kept to run by hand
    scheduleIn = function(_, s, f) if s < 1 then f() else ticks[#ticks + 1] = f end end,
    unschedule = function(_, f) for i = #ticks, 1, -1 do if ticks[i] == f then table.remove(ticks, i) end end end,
    nextTick = function(_, f) f() end,
    setDirty = function() end,
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
package.preload["docsettings"] = function() return {
    hasSidecarFile = function() return true end,
    open = function(_, path) return { readSetting = function(_, k)
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
    return string.format([[<entry><title>Book %d</title><author><name>Author %d</name></author>
<calibre:series>Saga</calibre:series><calibre:series_index>%d</calibre:series_index><summary>About book %d</summary>
<link rel="http://opds-spec.org/image/thumbnail" href="/thumb/b%d.epub" type="image/jpeg"/>
<link rel="http://opds-spec.org/image" href="/cover/b%d.epub" type="image/jpeg"/>
<link rel="http://opds-spec.org/acquisition" href="/download/b%d.epub" type="application/epub+zip" length="2048"/></entry>]], i, i, i, i, i, i, i)
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
    serverNow = function() return os.time() end,
    launcherEnabled = function(self) return self.launcher end,
    setLauncher = function(self, on) self.launcher = on end,
}
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
check(b2:statusLine(b2:currentLevel().entries[1]):match("read 42 %%"), "downloaded books show how far they were read")
b2:quickDownload(b2:currentLevel().entries[2])
check(shown[#shown].ok_text == "Read now", "a long press downloads and offers to read")
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
local Home = require("opdshelf_home")
local empty = Home:new{ plugin = plugin }
empty:refresh()
check(has(texts(empty), "^Nothing read yet$") and has(texts(empty), "^Open the library$"), "home without history invites to the library")
empty:onClose()
files["/books/other.epub"] = true
history[1] = { file = "/books/b1.epub", time = os.time() - 60 }
history[2] = { file = "/books/other.epub", time = os.time() - 7200 }
history[3] = { file = "/books/gone.epub", time = os.time() - 9000 }
requests = {}
local home = Home:new{ plugin = plugin }
plugin.home = home
home:show()
local seen = texts(home)
if os.getenv("DEBUG") then for _, t in ipairs(seen) do print("  text:", t) end end
check(#home.history == 2, "lists the books read on this device that are still there")
check(home.server_count == 40 and has(seen, "^40 books in your library$"), "says how many books the library has")
check(#home.recent == 12 and home.recent[1].title == "Book 1", "shows the newest books of the library")
check(has(seen, "^Title of b1%.epub$") and has(seen, "^42 %% read"), "continue reading: the last book and how far")
check(home.history[1].remote and has(seen, "^Further on Phone: 60 %%"), "and where another device went further")
check(has(seen, "^Recently added to the library$") and has(seen, "^Title of other%.epub$"), "both shelves")
check(has(seen, "✓ Book 1"), "books already downloaded are marked")
check(has(seen, "^%d+:%d%d · %d+ %a+$") and has(seen, "^87 %%$"), "a status bar with the time, date and battery")
for _, label in ipairs({ "Home", "Library", "Search", "Files", "Settings" }) do
    check(has(seen, "^" .. label .. "$"), "a tab for " .. label)
end
check(#ticks == 1, "the clock waits for the next minute")
local clock_before = home.clock_widget.text
home.clock_widget.text = "old"
table.remove(ticks)()
check(home.clock_widget.text == clock_before and #ticks == 1, "and then updates itself")
home:showMenu()
local home_menu = shown[#shown]
home_menu.buttons[2][1].callback()
check(plugin.launcher == true, "the menu turns opening at start on")
check(settings.home_recent and #settings.home_recent == 12, "keeps the shelf for when there is no connection")
home:openLibrary(false, home.recent_url, "Recently added")
check(plugin.library_opened.url == "https://books.example/opds/books", "see all opens the library there")
local before = #shown
home.browser:showDetails(home.recent[2])
check(#shown == before + 1, "a book of the shelf shows its details")
home:openBook("/books/other.epub")
check(home.closed and plugin.home == nil and shown[#shown].opened == "/books/other.epub", "opening a book leaves the home screen")
check(#ticks == 0, "and stops its clock")
-- Without a connection it shows what it kept
plugin.ready = function() return true end
local NetworkMgr = require("ui/network/manager")
NetworkMgr.isOnline = function() return false end
local offline = Home:new{ plugin = plugin }
offline:show()
check(#offline.recent == 12 and has(texts(offline), "^Offline"), "offline it shows the kept shelf and says so")
print("all good")

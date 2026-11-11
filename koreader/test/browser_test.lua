-- Builds every screen of the library browser against stand-ins for KOReader's widgets,
-- to catch mistakes in the code that lays them out.
-- Run from the repository root: luajit koreader/test/browser_test.lua koreader/opdshelf.koplugin
package.path = arg[1] .. "/?.lua;" .. package.path

local shown, closed, requests = {}, {}, {}
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
function Widget:disable() self.disabled = true end
function Widget:getInputText() return self.input_text or "hobbit" end
function Widget:onShowKeyboard() end

local widgets = {
    "ui/widget/container/bottomcontainer", "ui/widget/button", "ui/widget/buttondialog", "ui/widget/buttontable",
    "ui/widget/container/centercontainer", "ui/widget/confirmbox", "ui/widget/container/framecontainer",
    "ui/widget/horizontalgroup", "ui/widget/horizontalspan", "ui/widget/imagewidget", "ui/widget/infomessage",
    "ui/widget/container/inputcontainer", "ui/widget/inputdialog", "ui/widget/container/leftcontainer",
    "ui/widget/linewidget", "ui/widget/overlapgroup", "ui/widget/scrolltextwidget", "ui/widget/textboxwidget",
    "ui/widget/textwidget", "ui/widget/titlebar", "ui/widget/verticalgroup", "ui/widget/verticalspan",
    "ui/widget/pathchooser",
}
for _, name in ipairs(widgets) do
    package.preload[name] = function() return Widget:extend{ kind = name:match("([^/]+)$") } end
end
package.preload["ffi/blitbuffer"] = function() return { COLOR_WHITE = 0, COLOR_LIGHT_GRAY = 1 } end
package.preload["datastorage"] = function() return { getDataDir = function() return "/tmp/opdshelf-test" end } end
package.preload["device"] = function() return {
    hasKeys = function() return true end, isTouchDevice = function() return true end,
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
package.preload["ui/network/manager"] = function() return { isOnline = function() return true end, runWhenOnline = function(_, f) f() end } end
package.preload["ui/size"] = function() return {
    border = { thin = 1, button = 1 }, padding = { small = 2, default = 5, large = 10 }, line = { thin = 1 },
} end
package.preload["ui/uimanager"] = function() return {
    show = function(_, w) shown[#shown + 1] = w end,
    close = function(_, w) closed[#closed + 1] = w end,
    scheduleIn = function(_, _s, f) f() end,
    nextTick = function(_, f) f() end,
    setDirty = function() end,
    forceRePaint = function() end,
} end
package.preload["libs/libkoreader-lfs"] = function() return { attributes = function(path, what)
    if files[path] then return what and "file" or { mode = "file" } end
end } end
package.preload["logger"] = function() return { warn = function() end, dbg = function() end } end
package.preload["ffi/sha2"] = function() return { md5 = function(s) return (s:gsub("[^%w]", "")) end } end
package.preload["util"] = function() return {
    makePath = function() end,
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
    open = function() return { readSetting = function() return 0.42 end } end,
} end
package.preload["apps/reader/readerui"] = function() return { showReader = function(_, path) shown[#shown + 1] = { opened = path } end } end
package.preload["httpasync"] = function() return nil end

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
}

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
print("all good")

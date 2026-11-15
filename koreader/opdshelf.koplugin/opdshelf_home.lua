--[[--
OPDShelf's home screen: what to read next, at a glance.

- A compact header: the name with the menu, the clock, Wi-Fi and battery, and
  three small tiles: minutes read today on every device, the streak and the
  size of the library. A problem with the connection shows there in one line.
- Continue reading: the last book, its cover, how far it was read here and,
  when another device went further, where that one is.
- Started: the books started here or on other devices and not finished, the
  most recently read first, as a shelf (two rows on tall screens) with a full
  list one tap away.
- Recently added on the server, as a shelf of covers.
- Tabs for the library, its search, KOReader's file browser and a menu with
  KOReader's own screens (history, favorites, collections, file search, its
  top menu) and the plugin's settings.

The first paint uses what is on the device and what was kept from the last
connection; the server's answers repaint it once, and their covers once more.

It can replace KOReader's start screen (File browser → Settings → Start with).

@module opdshelf_home
--]]

local Blitbuffer = require("ffi/blitbuffer")
local Button = require("ui/widget/button")
local ButtonDialog = require("ui/widget/buttondialog")
local CenterContainer = require("ui/widget/container/centercontainer")
local Device = require("device")
local Font = require("ui/font")
local FrameContainer = require("ui/widget/container/framecontainer")
local Geom = require("ui/geometry")
local GestureRange = require("ui/gesturerange")
local HorizontalGroup = require("ui/widget/horizontalgroup")
local HorizontalSpan = require("ui/widget/horizontalspan")
local IconWidget = require("ui/widget/iconwidget")
local ImageWidget = require("ui/widget/imagewidget")
local InfoMessage = require("ui/widget/infomessage")
local InputContainer = require("ui/widget/container/inputcontainer")
local LeftContainer = require("ui/widget/container/leftcontainer")
local LineWidget = require("ui/widget/linewidget")
local NetworkMgr = require("ui/network/manager")
local OverlapGroup = require("ui/widget/overlapgroup")
local ProgressWidget = require("ui/widget/progresswidget")
local RightContainer = require("ui/widget/container/rightcontainer")
local Size = require("ui/size")
local TextBoxWidget = require("ui/widget/textboxwidget")
local TextWidget = require("ui/widget/textwidget")
local TitleBar = require("ui/widget/titlebar")
local UIManager = require("ui/uimanager")
local VerticalGroup = require("ui/widget/verticalgroup")
local VerticalSpan = require("ui/widget/verticalspan")
local lfs = require("libs/libkoreader-lfs")
local util = require("util")
local logger = require("logger")
local Browser = require("opdshelf_browser")
local Menu = require("opdshelf_menu")
local OPDS = require("opdshelf_opds")
local Sync = require("opdshelf_sync")
local LocalStats = require("opdshelf_localstats")
local T = require("ffi/util").template
local _ = require("opdshelf_i18n")
local Input = Device.input
local Screen = Device.screen

local Item = Browser.Item

-- Read this far, a book counts as finished (as on the server)
local FINISHED = 0.99
-- Secondary text: dark enough to read on e-ink
local GRAY = Blitbuffer.COLOR_DARK_GRAY or Blitbuffer.COLOR_GRAY_5

--[[--
The spacing of the plugin's screens, in dp, used for every distance on them:
- MARGIN (M): the left and right edge of everything on the screen.
- STEP_S (S): inside a block: cover, bar and caption; a title and its status.
- STEP_G (G): between a heading and what it heads, the tiles and the card,
  two rows of covers.
- STEP_L (L): between sections. What is left of the screen's height goes
  into these, evenly, so nothing is squeezed and no band stays empty.
- GAP: between tiles, and the least between two covers of a row; a row
  spreads its covers so the last one ends on the right margin.
- PAD: inside every box with a border (tiles, the card, pills, marks).
L = 2G = 4S, and PAD ≥ S.
--]]
local MARGIN = 16
local STEP_S = 4
local STEP_G = 8
local STEP_L = 16
local GAP = 12
local PAD = 8

-- Covers of books on this device, kept while KOReader runs (they take a while to extract)
local local_covers = {}

local function exists(path)
    return path and lfs.attributes(path, "mode") == "file"
end

local function fileTitle(path)
    return ((path:match("([^/]+)$") or path):gsub("%.[^.]+$", ""))
end

-- The first letters of the first two words, for a cover that is not there
local function initials(title)
    local out = {}
    for word in (title or ""):gmatch("[^%s%p]+") do
        local first = word:match("^[%z\1-\127\194-\244][\128-\191]*")
        if first then out[#out + 1] = first:upper() end
        if #out == 2 then break end
    end
    return #out > 0 and table.concat(out) or "?"
end

-- How tall `lines` lines of `face` are in a text box
local function linesHeight(face, lines, width)
    local probe = TextBoxWidget:new{ text = "Ag" .. string.rep("\nAg", lines - 1), face = face, width = width, for_measurement_only = true }
    local h = probe:getSize().h
    probe:free()
    return h
end

local function urlPath(path)
    return (path:gsub("[^%w%-%._~/]", function(c) return string.format("%%%02X", c:byte()) end))
end

local MIME = {
    epub = "application/epub+zip", pdf = "application/pdf", cbz = "application/x-cbz", cbr = "application/x-cbr",
    fb2 = "application/x-fictionbook+xml", mobi = "application/x-mobipocket-ebook", txt = "text/plain",
}

local Home = InputContainer:extend{
    plugin = nil,
    covers_fullscreen = true, -- hint for UIManager:_repaint()
}

-- ---------------------------------------------------------------------------
-- Data, kept apart from the widgets so it can be tested on its own

--- The path in the library of an OPDS entry of OPDShelf ("urn:opdshelf:book:<path>").
Home.libraryPath = Browser.libraryPath

--- An entry for a book of the library that the feed did not list, enough
--- for its details and its download.
function Home.entryFor(server, path, title)
    local ext = (path:match("%.(%w+)$") or "epub"):lower()
    return {
        id = "urn:opdshelf:book:" .. urlPath(path),
        title = title, authors = {}, categories = {}, is_book = true,
        acquisitions = { { href = server .. "/download/" .. urlPath(path), type = MIME[ext] or "application/epub+zip" } },
        thumbnail = server .. "/thumb/" .. urlPath(path),
        image = server .. "/cover/" .. urlPath(path),
    }
end

--[[--
The books the server knows as started or finished, from `GET /api/progress`
(`body`) and the library's "Continue reading" feed (`feed`, for authors and
covers; may be nil). Times are turned into this device's clock with `offset`
(server time minus device time).
--]]
function Home.remoteStarted(body, feed, offset, server)
    local out = {}
    if type(body) ~= "table" then return out end
    local marks = {}
    for _i, mark in ipairs(type(body.marks) == "table" and body.marks or {}) do
        if type(mark) == "table" and type(mark.document) == "string" then marks[mark.document] = mark end
    end
    local entries = {}
    for _i, entry in ipairs(type(feed) == "table" and feed.entries or {}) do
        local path = Home.libraryPath(entry)
        if path then entries[path] = entry end
    end
    local seen = {}
    for _i, book in ipairs(type(body.books) == "table" and body.books or {}) do
        local current = type(book) == "table" and book.current
        if type(current) == "table" and type(book.document) == "string" then
            local mark = marks[book.document]
            local percent = tonumber(current.percentage) or 0
            local at = tonumber(current.timestamp) or 0
            -- Marked as not finished after this position: not started any more
            local reset = mark and mark.state == "unread" and at <= (tonumber(mark.at) or 0)
            local finished = (mark and mark.state == "read") or percent >= FINISHED
            if not reset and (percent > 0 or finished) then
                local path = type(book.path) == "string" and book.path or nil
                local title = type(book.title) == "string" and book.title ~= "" and book.title or (path and fileTitle(path)) or "?"
                local entry = path and (entries[path] or (server and Home.entryFor(server, path, title)))
                seen[book.document] = true
                out[#out + 1] = {
                    document = book.document,
                    lib_path = path,
                    title = entry and entry.title ~= "" and entry.title or title,
                    authors = entry and entry.authors or {},
                    percent = percent,
                    finished = finished or nil,
                    time = at - (offset or 0),
                    device = type(current.device) == "string" and current.device or nil,
                    entry = entry,
                }
            end
        end
    end
    -- Finished by hand without a position: only to leave them out
    for document, mark in pairs(marks) do
        if mark.state == "read" and not seen[document] then
            out[#out + 1] = { document = document, finished = true, percent = 1, authors = {} }
        end
    end
    return out
end

--[[--
Started books, the most recently read first: KOReader's history on this
device merged with what the server knows (`remote`, from remoteStarted), as
Browser.mergeBooks merges them: one record per book, by document id, by the
library's file downloaded here (`localPathFor(entry)` says where it is, if it
is) or, when one side has no document id, by title and author; the newest
position wins. Finished books, books not started and `exclude` (the book in
the Continue card) are left out.
--]]
function Home.mergeStarted(locals, remote, exclude, localPathFor)
    local out = {}
    for _i, record in ipairs(Browser.mergeBooks(locals, remote, localPathFor)) do
        local excluded = exclude and ((record.path and record.path == exclude.path)
            or (record.document and record.document == exclude.document))
        if not excluded and not record.finished and record.percent > 0 and record.percent < FINISHED then
            out[#out + 1] = record
        end
    end
    table.sort(out, function(a, b)
        if (a.time or 0) ~= (b.time or 0) then return (a.time or 0) > (b.time or 0) end
        return (a.title or "") < (b.title or "")
    end)
    return out
end

--- The texts of the three tiles (nil when unknown) from the kept statistics,
--- and their captions when they say something else than the tile's name.
-- `today` is the date as os.date("%Y-%m-%d"): minutes from another day do not count.
-- Nothing yet is a dash with a word on what to do, not a big zero.
function Home.statTexts(stats, today)
    stats = type(stats) == "table" and stats or {}
    local texts = {}
    local seconds = tonumber(stats.today_seconds)
    if seconds and stats.date == today then
        -- Whole minutes, as the streak only counts a day from its first full minute
        local minutes = math.floor(seconds / 60)
        if seconds <= 0 then
            texts.today, texts.today_caption = "—", _("Not read today")
        elseif minutes < 60 then
            texts.today = T(_("%1 min"), minutes)
        elseif minutes % 60 == 0 then
            texts.today = T(_("%1 h"), math.floor(minutes / 60))
        else
            texts.today = T(_("%1 h %2 min"), math.floor(minutes / 60), minutes % 60)
        end
    end
    local streak = tonumber(stats.streak)
    if streak and streak <= 0 then
        texts.streak, texts.streak_caption = "—", _("Start today")
    elseif streak then
        texts.streak = streak == 1 and _("1 day") or T(_("%1 days"), streak)
    end
    if tonumber(stats.count) then texts.library = tostring(stats.count) end
    return texts
end

--- The spacing above, in pixels on this screen (also for the library's screens).
function Home.spacing()
    local function px(dp) return math.max(1, Screen:scaleBySize(dp)) end
    return { M = px(MARGIN), S = px(STEP_S), G = px(STEP_G), L = px(STEP_L), gap = px(GAP), pad = px(PAD) }
end

--[[--
`n` widths that fill `total` with `gap` between them, as equal as whole
pixels allow (the first ones get the pixel left over): the last one ends
exactly at the edge.
--]]
function Home.split(total, n, gap)
    local room = total - (n - 1) * gap
    local base = math.floor(room / n)
    local extra = room - base * n
    local out = {}
    for i = 1, n do out[i] = base + (i <= extra and 1 or 0) end
    return out
end

--- The `n - 1` gaps that spread `n` covers `cover_w` wide across `total`, so
--- the first starts at the left edge and the last ends at the right one.
function Home.gaps(total, n, cover_w)
    local out = {}
    if n < 2 then return out end
    local room = total - n * cover_w
    local base = math.floor(room / (n - 1))
    local extra = room - base * (n - 1)
    for i = 1, n - 1 do out[i] = base + (i <= extra and 1 or 0) end
    return out
end

-- ---------------------------------------------------------------------------
-- Screen

function Home:init()
    self.dimen = Geom:new{ x = 0, y = 0, w = Screen:getWidth(), h = Screen:getHeight() }
    if Device:hasKeys() then
        self.key_events.Close = { { Input.group.Back } }
    end
    if Device:isTouchDevice() then
        self.ges_events.Swipe = { GestureRange:new{ ges = "swipe", range = self.dimen } }
    end
    -- Details and downloads reuse the library's screens without showing it
    self.browser = Browser:new{ plugin = self.plugin }
    self.browser.on_open_book = function() self:onClose() end
    self.browser.on_download = function() self:refresh() end
    local state = self.plugin.state
    self.recent = state:readSetting("home_recent") or {}
    self.remote_started = state:readSetting("home_started") or {}
    self.stats = state:readSetting("home_stats") or { count = state:readSetting("home_count") }
    self.offline = not NetworkMgr:isOnline()
    self.sp = Home.spacing()
    self:loadLocal()
    self[1] = FrameContainer:new{ width = self.dimen.w, height = self.dimen.h, bordersize = 0, padding = 0,
        background = Blitbuffer.COLOR_WHITE, VerticalSpan:new{ width = 0 } }
end

function Home:show()
    UIManager:show(self, "flashui")
    self:refresh()
    UIManager:scheduleIn(0.3, function() self:loadRemote() end)
    self:scheduleClock()
end

-- The clock in the header changes on the minute, repainting only itself
function Home:scheduleClock()
    if self.clock_task then UIManager:unschedule(self.clock_task) end
    self.clock_task = function()
        self.clock_task = nil
        if self.closed then return end
        if self.clock_widget and self.clock_widget.setText then
            self.clock_widget:setText(self:clockText())
            local status = self.status_bar
            UIManager:setDirty(self, function() return "ui", status and status.dimen end)
        end
        self:scheduleClock()
    end
    UIManager:scheduleIn(60 - os.date("*t").sec, self.clock_task)
end

function Home:clockText()
    local twelve = G_reader_settings:isTrue("twelve_hour_clock")
    local time = os.date(twelve and "%I:%M %p" or "%H:%M"):gsub("^0(%d:)", "%1")
    local t = os.date("*t")
    return T(_("%1 · %2 %3"), time, t.day, _(os.date("%B")))
end

-- Clock and date, Wi-Fi and battery; a tap opens KOReader's menu, where they are set
function Home:statusGroup()
    local face = Font:getFace("smallinfofont", 15)
    self.clock_widget = TextWidget:new{ text = self:clockText(), face = face, max_width = math.floor(self.dimen.w * 0.4) }
    local h = self.clock_widget:getSize().h
    local group = HorizontalGroup:new{ align = "center", self.clock_widget }
    local wifi = false
    pcall(function() wifi = NetworkMgr:isWifiOn() end)
    if wifi then
        table.insert(group, HorizontalSpan:new{ width = Home.spacing().G })
        table.insert(group, IconWidget:new{ icon = "wifi", width = h, height = h })
    end
    local battery
    pcall(function()
        local powerd = Device:getPowerDevice()
        if Device:hasBattery() then
            battery = T(_("%1 %"), powerd:getCapacity()) .. (powerd:isCharging() and " +" or "")
        end
    end)
    if battery then
        table.insert(group, HorizontalSpan:new{ width = Home.spacing().G })
        table.insert(group, TextWidget:new{ text = battery, face = face })
    end
    return group
end

-- Name and menu on the left, status on the right; then the tiles and any problem.
-- Everything sits between the side margins: `w` is the width between them.
function Home:header(w)
    local sp = self.sp
    local name = TextWidget:new{ text = _("OPDShelf"), face = Font:getFace("tfont", 22), bold = true }
    -- The menu's three bars, drawn here so that they start exactly on the
    -- margin (KOReader's icon has empty room around its lines)
    local name_h = name:getSize().h
    local bar_w = math.floor(name_h * 0.8)
    local thick = math.max(2, Screen:scaleBySize(2))
    local bar_gap = math.floor((bar_w * 0.7 - 3 * thick) / 2)
    local bars = VerticalGroup:new{ align = "left" }
    for i = 1, 3 do
        if i > 1 then table.insert(bars, VerticalSpan:new{ width = bar_gap }) end
        table.insert(bars, LineWidget:new{ background = Blitbuffer.COLOR_BLACK, dimen = Geom:new{ w = bar_w, h = thick } })
    end
    local title = HorizontalGroup:new{ align = "center", bars, HorizontalSpan:new{ width = sp.G }, name }
    local status = self:statusGroup()
    local row_h = math.max(Screen:scaleBySize(40), name_h, status:getSize().h)
    local menu = Item:new{
        width = title:getSize().w + sp.G, height = row_h,
        callback = function() self:showMenu() end,
        LeftContainer:new{ dimen = Geom:new{ w = title:getSize().w + sp.G, h = row_h }, title },
    }
    -- Tapped a little left of the status too, but drawn up to the right margin
    local status_w = status:getSize().w + sp.G
    self.status_bar = Item:new{
        width = status_w,
        height = row_h,
        callback = function() self:callNative("menu", "onShowMenu", true) end,
        RightContainer:new{ dimen = Geom:new{ w = status_w, h = row_h }, status },
    }
    local top = OverlapGroup:new{
        dimen = Geom:new{ w = w, h = row_h },
        LeftContainer:new{
            dimen = Geom:new{ w = w, h = row_h },
            menu,
        },
        RightContainer:new{ dimen = Geom:new{ w = w, h = row_h }, self.status_bar },
    }
    top.home_block = "title"
    local group = VerticalGroup:new{
        align = "left",
        top,
        VerticalSpan:new{ width = sp.G },
        self:tiles(w),
    }
    local problem, fixable = self:problem()
    if problem then
        local face = Font:getFace("cfont", 15)
        local line = TextBoxWidget:new{
            text = problem, face = face, width = w, fgcolor = GRAY,
            height = linesHeight(face, 2, w), height_adjust = true, height_overflow_show_ellipsis = true,
        }
        table.insert(group, VerticalSpan:new{ width = sp.S })
        table.insert(group, fixable and Item:new{
            width = w, height = line:getSize().h,
            callback = function()
                self.plugin:editAccount(function() self:refreshAll() end)
            end,
            line,
        } or line)
    end
    return group
end

-- Today, Streak and Library: three frames as wide as each other, across the
-- whole width with one gap between them; "–" while unknown
function Home:tiles(w)
    local sp = self.sp
    local today = os.date("%Y-%m-%d")
    local yesterday = os.date("%Y-%m-%d", LocalStats.dayStart() - 12 * 3600)
    -- The server's numbers, with what was read here and has not reached it
    local stats = LocalStats.combine(self.stats, self.local_today, today, yesterday)
    local texts = Home.statTexts(stats, today)
    self.tile_texts = texts
    local gap = sp.gap
    local tiles = {
        { texts.today_caption or _("Today"), texts.today },
        { texts.streak_caption or _("Streak"), texts.streak },
        { _("Library"), texts.library, function() self:openLibrary() end },
    }
    local widths = Home.split(w, #tiles, gap)
    local border = Size.border.thin
    local value_face, caption_face = Font:getFace("smallinfofontbold", 19), Font:getFace("smallinfofont", 13)
    local probe = VerticalGroup:new{ TextWidget:new{ text = "Ag", face = value_face }, TextWidget:new{ text = "Ag", face = caption_face } }
    -- As tall as their texts and the box's padding, whatever they say
    local tile_h = probe:getSize().h + 2 * sp.pad + 2 * border
    local row = HorizontalGroup:new{ align = "top" }
    for i, tile in ipairs(tiles) do
        local tile_w = widths[i]
        local text_w = math.max(1, tile_w - 2 * sp.pad - 2 * border)
        local column = VerticalGroup:new{
            align = "center",
            TextWidget:new{ text = tile[2] or "–", face = value_face, max_width = text_w },
            TextWidget:new{ text = tile[1], face = caption_face, fgcolor = GRAY, max_width = text_w },
        }
        local frame = FrameContainer:new{
            width = tile_w, height = tile_h, bordersize = border, radius = Size.radius.window, padding = 0, margin = 0,
            background = Blitbuffer.COLOR_WHITE,
            CenterContainer:new{ dimen = Geom:new{ w = tile_w - 2 * border, h = tile_h - 2 * border }, column },
        }
        frame.home_block = "tile"
        table.insert(row, tile[3] and Item:new{ width = tile_w, height = tile_h, callback = tile[3], frame } or frame)
        if i < #tiles then table.insert(row, HorizontalSpan:new{ width = gap }) end
    end
    return row
end

-- A connection problem, and whether tapping it helps (it opens the account)
function Home:problem()
    if not self.plugin:ready() then
        return _("Not set up: tap here to connect to your OPDShelf server."), true
    elseif self.server_error then
        return self.server_error, true
    elseif self.offline then
        return _("Offline: showing what is on this device")
    elseif self.connected == false then
        return _("Cannot reach the server: showing what is on this device")
    end
end

--[[--
The tabs at the bottom, as in a phone app: equal cells across the whole
width, each with its icon and label in the middle and the same room above
and below. The labels get smaller rather than cut; the tab shown has a bold
label and a bar on top.
--]]
function Home:tabBar()
    local sp = self.sp
    local tabs = {
        { "home", _("Home"), function() self:refreshAll() end, true },
        { "book.opened", _("Library"), function() self:openLibrary() end },
        { "appbar.search", _("Search"), function() self:openLibrary(true) end },
        { "appbar.filebrowser", _("Files"), function() self:callNative() end },
        { "appbar.menu", _("More"), function() self:showMenu() end },
    }
    local W = self.dimen.w
    local widths = Home.split(W, #tabs, 0)
    local narrowest = widths[1]
    for _i, cell in ipairs(widths) do narrowest = math.min(narrowest, cell) end
    local icon = Screen:scaleBySize(28)
    -- The largest size at which every label, in bold, fits its cell
    local size = 14
    while size > 9 do
        local fits = true
        for _i, tab in ipairs(tabs) do
            local probe = TextWidget:new{ text = tab[2], face = Font:getFace("smallinfofont", size), bold = true }
            if probe:getSize().w > narrowest - 2 * sp.S then fits = false end
            probe:free()
        end
        if fits then break end
        size = size - 1
    end
    local face = Font:getFace("smallinfofont", size)
    local label_h = TextWidget:new{ text = "Ag", face = face }:getSize().h
    local marker_h = Screen:scaleBySize(5)
    local block_h = icon + sp.S + label_h
    local tab_h = block_h + 2 * sp.G + marker_h
    local row = HorizontalGroup:new{ align = "top" }
    for i, tab in ipairs(tabs) do
        local active = tab[4]
        local tab_w = widths[i]
        local column = VerticalGroup:new{
            align = "center",
            IconWidget:new{ icon = tab[1], width = icon, height = icon },
            VerticalSpan:new{ width = sp.S },
            TextWidget:new{ text = tab[2], face = face, bold = active, max_width = tab_w - sp.S },
        }
        table.insert(row, Item:new{
            width = tab_w,
            height = tab_h,
            callback = tab[3],
            OverlapGroup:new{
                dimen = Geom:new{ w = tab_w, h = tab_h },
                -- Under the marker's height, the block in the middle of what is left
                VerticalGroup:new{
                    VerticalSpan:new{ width = marker_h },
                    CenterContainer:new{ dimen = Geom:new{ w = tab_w, h = tab_h - marker_h }, column },
                },
                active and CenterContainer:new{
                    dimen = Geom:new{ w = tab_w, h = marker_h },
                    LineWidget:new{ background = Blitbuffer.COLOR_BLACK, dimen = Geom:new{ w = math.floor(tab_w * 0.6), h = marker_h } },
                } or VerticalSpan:new{ width = 0 },
            },
        })
    end
    local bar = VerticalGroup:new{
        align = "center",
        LineWidget:new{ background = Blitbuffer.COLOR_LIGHT_GRAY, dimen = Geom:new{ w = W, h = Size.line.thin } },
        row,
    }
    bar.home_block = "tabs"
    return bar
end

function Home:refreshAll()
    self:loadLocal()
    self:refresh()
    self:loadRemote()
end

--[[--
The books opened on this device, newest first, with what their settings say
(the library's screens shown from here use the same), and what KOReader's
statistics know: the time read today here, and when each book was last read.
A page read counts from then on, so a book opened long ago and read since
says so, rather than when it was opened.
--]]
function Home:loadLocal()
    local ok, ReadHistory = pcall(require, "readhistory")
    if ok and type(ReadHistory) == "table" and ReadHistory.reload then pcall(ReadHistory.reload, ReadHistory) end
    self.browser:forgetLocal()
    self.history = self.browser:localBooks()
    local md5s = {}
    for _i, book in ipairs(self.history) do
        if book.document then md5s[#md5s + 1] = book.document end
    end
    local stats = LocalStats.read(LocalStats.dayStart(), md5s)
    self.local_today = stats and stats.today or nil
    for _i, book in ipairs(self.history) do
        local at = stats and book.document and stats.last[book.document]
        if at and (book.time_guess or not book.time or at > book.time) then
            book.time, book.time_guess = at, false
        end
    end
end

-- The started books of the shelf, without the one in the Continue card;
-- `all` keeps that one too, for the full list
function Home:startedBooks(all)
    return Home.mergeStarted(self.history, self.remote_started, not all and self.history[1] or nil, function(entry)
        local path, downloaded = self.browser:localFile(entry)
        return downloaded and path or nil
    end)
end

-- Saves what the next start shows before the server answers
function Home:keep()
    local state = self.plugin.state
    state:saveSetting("home_recent", self.recent)
    state:saveSetting("home_started", self.remote_started)
    state:saveSetting("home_stats", self.stats)
    state:flush()
end

--- The server's part: statistics, the newest books, where every book is and
--- the covers. One repaint when it is all there, one more for the covers.
function Home:loadRemote()
    if self.closed then return end
    if not self.plugin:ready() or not NetworkMgr:isOnline() then
        local was = self.offline
        self.offline = not NetworkMgr:isOnline()
        if was ~= self.offline then self:refresh() end
        self:fetchCovers()
        return
    end
    self.offline = false
    local client = self.plugin.client
    local code, body = client:fetch(client.server .. "/")
    local root = code == 200 and OPDS.parse(body, client.server .. "/")
    if not root then
        self.connected = false
        self.server_error = code == 401 and _("The server did not accept the username or password. Tap here to check them.") or nil
        self:refresh()
        self:fetchCovers()
        return
    end
    self.connected = true
    self.server_error = nil
    self.browser.root = root
    for _i, entry in ipairs(root.entries) do
        local count = entry.summary and tonumber(entry.summary:match("^(%d+)"))
        if count then
            self.stats.count = count
            self.recent_url = entry.navigation
            break
        end
    end
    local url = self.recent_url or (client.server .. "/opds/books")
    code, body = client:fetch(url)
    local feed = code == 200 and OPDS.parse(body, url)
    if feed then
        self.recent = {}
        for _i, entry in ipairs(feed.entries) do
            -- More than a row: the books already on the screen above are left out of it
            if entry.is_book and #self.recent < 20 then self.recent[#self.recent + 1] = entry end
        end
    end
    -- Time read today on every device, and the streak, on this device's day.
    -- What was read here and not sent yet goes first, so it counts.
    if self.plugin.flush then pcall(self.plugin.flush, self.plugin, false) end
    local c, today = client:getToday(os.date("%Y-%m-%d"))
    if c == 200 and type(today) == "table" and tonumber(today.seconds) then
        self.stats.date = os.date("%Y-%m-%d")
        self.stats.today_seconds = tonumber(today.seconds)
        self.stats.streak = tonumber(today.streak) or self.stats.streak
    end
    -- Where every book is, on every device
    local progress
    c, progress = client:getLibraryProgress()
    if c == 200 and type(progress) == "table" then
        self.plugin:noteServerTime(progress)
        local reading_url = client.server .. "/opds/reading"
        local rc, reading_body = client:fetch(reading_url)
        local reading = rc == 200 and OPDS.parse(reading_body, reading_url) or nil
        self.remote_started = Home.remoteStarted(progress, reading, self.plugin.server_offset, client.server)
        -- The details and hold menus shown from here say the same
        self.browser.remote = self.remote_started
        self:noteFurther(progress)
    end
    self:keep()
    if self.closed then return end
    self:refresh()
    self:fetchCovers()
end

-- Where the book to continue was left on other devices, when further on
function Home:noteFurther(progress)
    local book = self.history[1]
    if not book or not book.document then return end
    for _i, entry in ipairs(type(progress.books) == "table" and progress.books or {}) do
        local current = type(entry) == "table" and entry.document == book.document and entry.current
        if type(current) == "table" then
            if current.device_id ~= self.plugin.device_id
                    and (tonumber(current.percentage) or 0) > (book.percent or 0) + 0.001 then
                book.remote = current
            end
            return
        end
    end
end

-- Thumbnails of the books shown, then covers of the books on this device, then one repaint
function Home:fetchCovers()
    UIManager:scheduleIn(0.1, function()
        if self.closed then return end
        local got = self:downloadThumbnails(self.shown_entries or {})
        for _i, book in ipairs(self.shown_local or {}) do
            if self:extractCover(book) then got = got + 1 end
        end
        if got > 0 and not self.closed then self:refresh() end
    end)
end

function Home:downloadThumbnails(entries)
    local got = 0
    if not self.plugin:ready() or self.offline then return got end
    for _i, entry in ipairs(entries) do
        local path = self.browser:coverPath(entry.thumbnail)
        if path and not exists(path) and not entry.cover_failed then
            local code, data = self.plugin.client:raw("GET", entry.thumbnail, nil, nil, nil, { 3, 6 })
            if code == 200 and Browser.imageData(data) then
                local file = io.open(path, "wb")
                if file then file:write(data) file:close() got = got + 1 end
            else
                entry.cover_failed = true
                if not code then break end -- offline: do not wait for each one
            end
        end
    end
    return got
end

-- The cover of a book on this device: CoverBrowser's cache, the one saved when
-- it was downloaded from the library, or taken from the book itself (once)
function Home:localCover(book)
    local cached = local_covers[book.path]
    if cached then return cached end
    local saved = (self.plugin.state:readSetting("covers") or {})[book.path]
    if exists(saved) and Browser.isImage(saved) then return saved end
    local ok, BookInfoManager = pcall(require, "bookinfomanager")
    if ok and type(BookInfoManager) == "table" and BookInfoManager.getBookInfo then
        local okinfo, info = pcall(BookInfoManager.getBookInfo, BookInfoManager, book.path, true)
        -- A blank picture (an empty first page) is no cover: the initials say more
        if okinfo and info and info.cover_bb and not Browser.blankImage(info.cover_bb) then
            local_covers[book.path] = { bb = info.cover_bb }
            return local_covers[book.path]
        end
    end
    return nil
end

function Home:extractCover(book)
    if not book.path or self:localCover(book) or book.no_cover then return false end
    local ok, bb = pcall(function()
        local FileManagerBookInfo = require("apps/filemanager/filemanagerbookinfo")
        return FileManagerBookInfo:getCoverImage(nil, book.path)
    end)
    if ok and bb and not Browser.blankImage(bb) then
        local_covers[book.path] = { bb = bb }
        return true
    end
    book.no_cover = true
    return false
end

-- The cover of a started book: from the book on this device, else the library's thumbnail
function Home:recordCover(record)
    local cover = record.on_device and record.path and self:localCover(record)
    if not cover and record.entry then cover = self.browser:cachedCover(record.entry) end
    return cover
end

-- ---------------------------------------------------------------------------
-- Pieces

-- A cover in a thin frame, or the title's initials on light gray while there is none.
-- `badge` is a small mark in the top corner.
function Home:cover(title, cover, w, h, badge)
    local border = Size.border.thin
    local iw, ih = math.max(1, w - 2 * border), math.max(1, h - 2 * border)
    local inner
    -- Anything that is not a picture shows the initials, never a blank tile
    if type(cover) == "table" and not cover.bb then cover = nil end
    if type(cover) == "table" and cover.bb then
        inner = ImageWidget:new{ image = cover.bb, image_disposable = false, width = iw, height = ih, scale_factor = 0 }
    elseif cover then
        inner = ImageWidget:new{ file = cover, width = iw, height = ih, scale_factor = 0, file_do_cache = false }
    else
        inner = TextWidget:new{
            text = initials(title),
            face = Font:getFace("tfont", math.max(10, math.floor(w * 10 / Screen:scaleBySize(40)))),
            bold = true, fgcolor = GRAY, max_width = iw,
        }
    end
    local frame = FrameContainer:new{
        width = w, height = h, bordersize = border, padding = 0, margin = 0,
        background = cover and Blitbuffer.COLOR_WHITE or Blitbuffer.COLOR_LIGHT_GRAY,
        CenterContainer:new{ dimen = Geom:new{ w = iw, h = ih }, inner },
    }
    if not badge then return frame end
    -- One mark in the top corner: a small round one with a sign, the same size on every cover
    local d = math.max(Screen:scaleBySize(12), math.min(Screen:scaleBySize(22), math.floor(w * 0.3)))
    local mark = FrameContainer:new{
        width = d, height = d, bordersize = border, radius = math.floor(d / 2), margin = 0, padding = 0,
        background = Blitbuffer.COLOR_WHITE,
        CenterContainer:new{
            dimen = Geom:new{ w = d - 2 * border, h = d - 2 * border },
            TextWidget:new{ text = badge, face = Font:getFace("smallinfofontbold", math.max(8, math.floor(13 * d / Screen:scaleBySize(22)))) },
        },
    }
    local inset = Home.spacing().S
    return OverlapGroup:new{
        dimen = Geom:new{ w = w, h = h },
        frame,
        RightContainer:new{
            dimen = Geom:new{ w = w, h = d + inset },
            VerticalGroup:new{ VerticalSpan:new{ width = inset }, HorizontalGroup:new{ mark, HorizontalSpan:new{ width = inset } } },
        },
    }
end

--- The marks on covers: ↓ for a book to download, ✓ for a finished one; a
--- book on this device needs none.
function Home.badgeFor(on_device, finished)
    if finished then return "✓" end
    if not on_device then return "↓" end
    return nil
end

-- A heading in one style: the label in bold, "See all ›" smaller and in gray
-- on the right margin, a thin rule under them and then G to what they head
function Home:sectionHeader(w, text, link, callback)
    local sp = self.sp
    local label = TextWidget:new{ text = text, face = Font:getFace("smallinfofontbold", 17), max_width = math.floor(w * 0.65) }
    local h = label:getSize().h
    local group = OverlapGroup:new{ dimen = Geom:new{ w = w, h = h }, LeftContainer:new{ dimen = Geom:new{ w = w, h = h }, label } }
    if link then
        local more = TextWidget:new{ text = link .. " ›", face = Font:getFace("smallinfofont", 14), fgcolor = GRAY,
            max_width = math.floor(w * 0.33) }
        local more_w = more:getSize().w
        -- On the label's baseline, not in the middle of its height
        local drop = 0
        if label.getBaseline and more.getBaseline then
            drop = math.max(0, math.min(h - more:getSize().h, label:getBaseline() - more:getBaseline()))
        end
        local placed = VerticalGroup:new{ align = "right", VerticalSpan:new{ width = drop }, more,
            VerticalSpan:new{ width = h - drop - more:getSize().h } }
        -- Tapped a little around it, drawn up to the margin
        local tap_w = more_w + sp.G
        table.insert(group, RightContainer:new{
            dimen = Geom:new{ w = w, h = h },
            Item:new{ width = tap_w, height = h, callback = callback,
                RightContainer:new{ dimen = Geom:new{ w = tap_w, h = h }, placed } },
        })
    end
    local header = VerticalGroup:new{
        align = "left",
        group,
        VerticalSpan:new{ width = sp.S },
        LineWidget:new{ background = Blitbuffer.COLOR_LIGHT_GRAY, dimen = Geom:new{ w = w, h = Size.line.thin } },
        VerticalSpan:new{ width = sp.G },
    }
    header.home_block = "section"
    return header
end

-- The progress line under a book
local function bar(width, height, percent)
    return ProgressWidget:new{
        width = width, height = height, percentage = percent or 0,
        margin_h = 0, margin_v = 0, radius = 0, bordersize = 0,
        bgcolor = Blitbuffer.COLOR_LIGHT_GRAY, fillcolor = Blitbuffer.COLOR_BLACK,
    }
end

-- "42 % · 3 days ago · 120 pages left"; `long` says when in words, and
-- `pages` (as `long` when not given) adds the pages left
function Home:progressText(book, long, pages)
    if pages == nil then pages = long end
    local parts = {}
    if book.finished then
        parts[1] = _("Finished")
    else
        parts[1] = T(_("%1 %"), math.floor((book.percent or 0) * 100 + 0.5))
    end
    if book.time then
        parts[#parts + 1] = long and Sync.ago(book.time, os.time(), _, T) or Sync.shortAgo(book.time, os.time(), _, T)
    end
    if pages and book.pages and book.pages > 0 and not book.finished then
        local left = math.floor(book.pages * (1 - (book.percent or 0)) + 0.5)
        if left > 0 then parts[#parts + 1] = left == 1 and _("1 page left") or T(_("%1 pages left"), left) end
    end
    return table.concat(parts, " · ")
end

--[[--
Continue reading: the card on top, `w` wide, as tall as its cover and text
need, up to `max_h`. The cover is large on the left and the text fills the
rest of the card: title and author at the top, the bar, the status line and
"Continue ›" (at the right edge, on the status line) at the bottom.
`tall` fills `max_h` with a large cover over the text, for a column of its
own; the third value says whether it did.
--]]
function Home:continueCard(w, max_h, tall)
    local sp = self.sp
    local border = Size.border.thin
    local pad = sp.pad
    local book = self.history[1]
    local function frame(content, h, callback, hold_callback)
        local card = FrameContainer:new{
            width = w, height = h, padding = 0, margin = 0, bordersize = border, radius = Size.radius.window,
            background = Blitbuffer.COLOR_WHITE,
            content,
        }
        card.home_block = "card"
        if not callback then return card end
        return Item:new{ width = w, height = h, callback = callback, hold_callback = hold_callback, card }
    end
    local inner_w = w - 2 * border - 2 * pad
    if not book then
        local small = Font:getFace("cfont", 15)
        local inner = VerticalGroup:new{
            align = "center",
            TextWidget:new{ text = _("Nothing read yet"), face = Font:getFace("tfont", 20), bold = true, max_width = inner_w },
            VerticalSpan:new{ width = sp.S },
            TextBoxWidget:new{
                text = _("Find a book in your OPDShelf library and download it to start."),
                face = small, width = inner_w, alignment = "center", fgcolor = GRAY,
                height = linesHeight(small, 2, inner_w), height_adjust = true, height_overflow_show_ellipsis = true,
            },
            VerticalSpan:new{ width = sp.G },
            Button:new{
                text = _("Open the library"), width = math.floor(w * 0.5), show_parent = self,
                callback = function() self:openLibrary() end,
            },
        }
        local h = math.min(max_h, inner:getSize().h + 2 * pad + 2 * border)
        return frame(CenterContainer:new{ dimen = Geom:new{ w = w - 2 * border, h = h - 2 * border }, inner }, h), h
    end

    local inner_max = max_h - 2 * border - 2 * pad
    local title_face = Font:getFace("tfont", 20)
    local small = Font:getFace("cfont", 15)
    local remote_text = book.remote and T(_("Further on %1: %2 % (%3). Open it to continue from there."),
        (type(book.remote.device) == "string" and book.remote.device ~= "") and book.remote.device or _("another device"),
        math.floor((tonumber(book.remote.percentage) or 0) * 100 + 0.5),
        Sync.ago(book.remote.timestamp, self.plugin:serverNow(), _, T))

    -- Continue ›, as a pill at the right edge of the status line, on its middle
    local function statusRow(text_w)
        local continue = FrameContainer:new{
            bordersize = 0, radius = Size.radius.window, margin = 0,
            padding = sp.S, padding_left = pad, padding_right = pad,
            background = Blitbuffer.COLOR_BLACK,
            TextWidget:new{ text = _("Continue") .. " ›", face = Font:getFace("smallinfofontbold", 15), fgcolor = Blitbuffer.COLOR_WHITE },
        }
        local row_h = continue:getSize().h
        -- The whole line when it fits; else without the pages left, then when in a few letters
        local status_w = math.max(1, text_w - continue:getSize().w - sp.G)
        local status_text
        for _i, form in ipairs({ { true, true }, { true, false }, { false, false } }) do
            status_text = self:progressText(book, form[1], form[2])
            local probe = TextWidget:new{ text = status_text, face = small }
            local fits = probe:getSize().w <= status_w
            probe:free()
            if fits then break end
        end
        return OverlapGroup:new{
            dimen = Geom:new{ w = text_w, h = row_h },
            LeftContainer:new{
                dimen = Geom:new{ w = text_w, h = row_h },
                TextWidget:new{ text = status_text, face = small, max_width = status_w },
            },
            RightContainer:new{ dimen = Geom:new{ w = text_w, h = row_h }, continue },
        }
    end
    -- The text `text_w` wide in two parts (what the book is; how far it was
    -- read), from everything to the least, until they are `budget` tall at most
    local function details(text_w, budget)
        local top, bottom
        for level = 0, 3 do
            if top then top:free() bottom:free() end
            top = VerticalGroup:new{ align = "left" }
            table.insert(top, TextBoxWidget:new{
                text = book.title, face = title_face, bold = true, width = text_w,
                height = linesHeight(title_face, level < 2 and 2 or 1, text_w), height_adjust = true, height_overflow_show_ellipsis = true,
            })
            if #book.authors > 0 and level < 2 then
                table.insert(top, VerticalSpan:new{ width = sp.S })
                table.insert(top, TextWidget:new{ text = table.concat(book.authors, ", "), face = Font:getFace("cfont", 16), fgcolor = GRAY, max_width = text_w })
            end
            bottom = VerticalGroup:new{
                align = "left",
                ProgressWidget:new{
                    width = text_w, height = Screen:scaleBySize(7), percentage = book.finished and 1 or (book.percent or 0),
                    margin_h = 0, margin_v = 0, radius = Screen:scaleBySize(3), bordersize = Size.border.thin,
                    fillcolor = Blitbuffer.COLOR_BLACK,
                },
                VerticalSpan:new{ width = sp.S },
                statusRow(text_w),
            }
            if remote_text and level < 3 then
                table.insert(bottom, VerticalSpan:new{ width = sp.S })
                table.insert(bottom, TextBoxWidget:new{
                    text = remote_text, face = small, bold = true, width = text_w,
                    height = linesHeight(small, level == 0 and 2 or 1, text_w), height_adjust = true, height_overflow_show_ellipsis = true,
                })
            end
            if top:getSize().h + sp.G + bottom:getSize().h <= budget then break end
        end
        return top, bottom
    end
    -- The two parts in a column `h` tall: one at the top, the other at the
    -- bottom; between them, the book's description in the whole lines that
    -- fit G under the first and G over the second, so no empty band is left
    local description = type(book.description) == "string" and util.htmlToPlainTextIfHtml(book.description) or nil
    if description then description = description:gsub("%s+", " "):gsub("^ ", ""):gsub(" $", "") end
    local function stack(top, bottom, h, text_w)
        local between = math.max(sp.G, h - top:getSize().h - bottom:getSize().h)
        local group = VerticalGroup:new{ align = "left", top }
        if description and description ~= "" then
            local line_h = linesHeight(small, 2, text_w) - linesHeight(small, 1, text_w)
            local lines = math.floor((between - 2 * sp.G) / line_h)
            if lines >= 1 then
                local text = TextBoxWidget:new{
                    text = description, face = small, fgcolor = GRAY, width = text_w,
                    height = linesHeight(small, lines, text_w), height_overflow_show_ellipsis = true,
                }
                table.insert(group, VerticalSpan:new{ width = sp.G })
                table.insert(group, text)
                between = between - sp.G - text:getSize().h
            end
        end
        table.insert(group, VerticalSpan:new{ width = between })
        table.insert(group, bottom)
        return group
    end
    local open = function() self:openBook(book.path) end
    local hold = function() self:localActions(book) end
    local cover = self:localCover(book)

    if tall then
        -- A column of its own: the cover large on top, the text under it, as tall as the column
        local min_cover = Screen:scaleBySize(120)
        local top, bottom = details(inner_w, inner_max - min_cover - sp.G)
        local text_h = top:getSize().h + sp.G + bottom:getSize().h
        local cover_h = inner_max - text_h - sp.G
        local cover_w = math.min(inner_w, math.floor(cover_h / Browser.COVER_RATIO))
        cover_h = math.min(cover_h, math.floor(cover_w * Browser.COVER_RATIO))
        if cover_h >= min_cover then
            local content = VerticalGroup:new{
                align = "left",
                CenterContainer:new{ dimen = Geom:new{ w = inner_w, h = cover_h }, self:cover(book.title, cover, cover_w, cover_h) },
                VerticalSpan:new{ width = sp.G },
                stack(top, bottom, inner_max - cover_h - sp.G, inner_w),
            }
            return frame(HorizontalGroup:new{ HorizontalSpan:new{ width = pad },
                VerticalGroup:new{ align = "left", VerticalSpan:new{ width = pad }, content } }, max_h, open, hold), max_h, true
        end
        top:free()
        bottom:free()
    end

    -- Beside the text: a large cover, as tall as the card allows
    local cover_w = math.max(Screen:scaleBySize(70), math.min(Screen:scaleBySize(180), math.floor(w * 0.26)))
    local cover_h = math.min(inner_max, math.floor(cover_w * Browser.COVER_RATIO))
    cover_w = math.floor(cover_h / Browser.COVER_RATIO)
    local text_w = inner_w - cover_w - pad
    local top, bottom = details(text_w, inner_max)
    -- As tall as the cover, or as the text when that needs more
    local content_h = math.min(inner_max, math.max(cover_h, top:getSize().h + sp.G + bottom:getSize().h))
    local h = content_h + 2 * pad + 2 * border
    local content = HorizontalGroup:new{
        align = "top",
        HorizontalSpan:new{ width = pad },
        self:cover(book.title, cover, cover_w, cover_h),
        HorizontalSpan:new{ width = pad },
        stack(top, bottom, content_h, text_w),
    }
    return frame(VerticalGroup:new{ align = "left", VerticalSpan:new{ width = pad }, content }, h, open, hold), h
end

-- One row of covers across the whole width `L.w`: the first at the left
-- margin, the last at the right one, the gaps between them as equal as
-- pixels allow. A shorter row keeps the same places, from the left. `kind`
-- is "started" (with progress and a line under the title) or "recent" (with
-- the title only when there is room).
function Home:shelf(items, L, kind)
    local row = HorizontalGroup:new{ align = "top" }
    row.home_block = "shelf"
    row.home_count, row.home_items = L.count, #items
    local row_h = kind == "started" and L.started_row_h or L.recent_row_h
    local gaps = Home.gaps(L.w, L.count, L.cover_w)
    for i, item in ipairs(items) do
        local column = VerticalGroup:new{ align = "center", self:cover(item.title, item.cover, L.cover_w, L.cover_h, item.badge) }
        if kind == "started" then
            table.insert(column, VerticalSpan:new{ width = L.space })
            table.insert(column, bar(L.cover_w, L.bar_h, item.percent))
        end
        if kind == "started" or L.captions then
            table.insert(column, VerticalSpan:new{ width = L.space })
            table.insert(column, TextBoxWidget:new{
                text = item.title, face = L.caption_face, width = L.cover_w, height = L.caption_h,
                height_overflow_show_ellipsis = true, alignment = "center",
            })
        end
        if kind == "started" then
            table.insert(column, CenterContainer:new{
                dimen = Geom:new{ w = L.cover_w, h = L.meta_h },
                TextWidget:new{ text = item.meta, face = L.meta_face, fgcolor = GRAY, max_width = L.cover_w },
            })
        end
        table.insert(row, Item:new{
            width = L.cover_w,
            height = row_h,
            callback = item.callback,
            hold_callback = item.hold_callback,
            LeftContainer:new{ dimen = Geom:new{ w = L.cover_w, h = row_h }, VerticalGroup:new{ align = "center", column } },
        })
        if i < #items then table.insert(row, HorizontalSpan:new{ width = gaps[i] }) end
    end
    return row
end

-- A line in place of an empty shelf
function Home:emptyLine(w, h, text)
    return CenterContainer:new{
        dimen = Geom:new{ w = w, h = h },
        TextBoxWidget:new{ text = text, face = Font:getFace("cfont", 15), width = w, alignment = "center", fgcolor = GRAY,
            height = h, height_overflow_show_ellipsis = true },
    }
end

--- The line under a started book's cover: "43 % · 22 h ago", or only how far
--- when that does not fit in `width` with `face`.
function Home:caption(record, face, width)
    local percent = T(_("%1 %"), math.floor((record.percent or 0) * 100 + 0.5))
    if not record.time then return percent end
    local text = percent .. " · " .. Sync.ago(record.time, os.time(), _, T)
    local probe = TextWidget:new{ text = text, face = face }
    local fits = probe:getSize().w <= width
    probe:free()
    return fits and text or percent
end

function Home:startedItem(record, L)
    return {
        title = record.title,
        cover = self:recordCover(record),
        percent = record.percent,
        meta = L and self:caption(record, L.meta_face, L.cell_w) or self:progressText(record, false),
        badge = Home.badgeFor(record.on_device, record.finished),
        callback = function() self:openRecord(record) end,
        hold_callback = function() self:recordActions(record) end,
    }
end

function Home:recentItems(count)
    local items = {}
    for i = 1, math.min(count, #self.recent_shown) do
        local entry = self.recent_shown[i]
        local state = self.browser:progressFor(entry)
        items[#items + 1] = {
            title = entry.title,
            cover = self.browser:cachedCover(entry),
            badge = Home.badgeFor(self.browser:onDevice(entry) ~= nil, state and state.finished),
            callback = function() self.browser:showDetails(entry) end,
            hold_callback = function() self.browser:bookActions(entry) end,
        }
    end
    return items
end

--[[--
The newest books of the library without those already on this screen: the
book of the Continue card and the started books. The same book is found by
the library's path, where it is on this device, its document id or, when
one has none, its title and author.
--]]
function Home:recentNotShown()
    local shown = {}
    if self.history[1] then shown[1] = self.history[1] end
    for _i, record in ipairs(self.started or {}) do shown[#shown + 1] = record end
    local out = {}
    for _i, entry in ipairs(self.recent or {}) do
        local lib = Home.libraryPath(entry)
        local here = self.browser:onDevice(entry)
        local known = self.browser:bookFor(entry)
        local document = known and known.document
        local repeated = false
        for _j, record in ipairs(shown) do
            if (lib and (record.lib_path == lib or Home.libraryPath(record.entry) == lib))
                    or (here and record.path == here)
                    or (document and document == record.document)
                    or ((not record.document or not document) and Browser.sameBook(entry, record)) then
                repeated = true
                break
            end
        end
        if not repeated then out[#out + 1] = entry end
    end
    return out
end

--[[--
Where everything goes in a column `w` wide and `avail` tall, holding the
parts asked for (`parts.card`, `parts.started`, `parts.recent`). The card
takes what its text needs, up to `parts.card_max`; the shelves get covers as
wide as their columns allow. When that does not fit, the second row of
Started goes first (it is only added when there is room), then the covers
get smaller: Recently added keeps its titles while its covers stay a good
size (always with `parts.captions`), else it shows covers only; when they
would be tiny, Recently added goes. What is left on a tall screen goes to
more rows of Started, up to `parts.max_rows`, and what is still left is
spread between the sections and under the last one (or between the rows,
in a column of only Started), so the column fills its height.
--]]
function Home:layout(w, avail, parts)
    local sp = self.sp
    local L = { w = w, space = sp.S, bar_h = Screen:scaleBySize(4), parts = parts, gap_l = sp.L, gap_row = sp.G }
    local gap = sp.gap
    L.gap_h = gap
    L.count = math.max(3, math.min(8, math.floor((w + gap) / (Screen:scaleBySize(105) + gap))))
    local natural_w = math.floor((w - (L.count - 1) * gap) / L.count)
    L.caption_face = Font:getFace("cfont", 14)
    L.meta_face = Font:getFace("smallinfofont", 13)
    L.caption_h = linesHeight(L.caption_face, 2, natural_w)
    L.meta_h = TextWidget:new{ text = "0 %", face = L.meta_face }:getSize().h
    L.section_h = self:sectionHeader(w, "Ag", "Ag"):getSize().h
    L.empty_h = linesHeight(Font:getFace("cfont", 15), 2, w)
    local natural = math.floor(natural_w * Browser.COVER_RATIO)
    L.natural = natural
    local small = Screen:scaleBySize(70)
    local has_started = #self.started > 0
    local has_recent = #self.recent_shown > 0
    local started_extra = 2 * L.space + L.bar_h + L.caption_h + L.meta_h
    local function recentExtra(captions) return captions and (L.space + L.caption_h) or 0 end
    L.card_h = 0
    if parts.card then
        L.card, L.card_h, L.card_tall = self:continueCard(w, math.max(Screen:scaleBySize(110), parts.card_max), parts.tall)
    end
    L.started, L.recent = parts.started, parts.recent
    -- The blocks of the column, one under the other, L apart; rows of Started G apart
    local function total(rows, cover_h, captions, recent)
        local t, blocks = 0, 0
        local function add(h)
            if blocks > 0 then t = t + sp.L end
            t, blocks = t + h, blocks + 1
        end
        if L.card then add(L.card_h) end
        if L.started then
            add(L.section_h + (has_started and (rows * (cover_h + started_extra) + (rows - 1) * sp.G) or L.empty_h))
        end
        if recent then
            add(L.section_h + (has_recent and (cover_h + recentExtra(captions)) or L.empty_h))
        end
        return t, blocks
    end
    local function covers(rows, recent)
        return (L.started and has_started and rows or 0) + (recent and has_recent and 1 or 0)
    end
    L.rows, L.cover_h, L.captions = 1, natural, true
    if total(1, natural, true, L.recent) > avail then
        local n = covers(1, L.recent)
        local with_captions = n > 0 and math.floor((avail - total(1, 0, true, L.recent)) / n) or natural
        if parts.captions or not L.recent or with_captions >= math.max(small, natural * 0.6) then
            L.cover_h = with_captions
        else
            L.captions = false
            L.cover_h = n > 0 and math.floor((avail - total(1, 0, false, L.recent)) / n) or natural
        end
        if L.cover_h < small and L.started and L.recent then
            -- Too small for two shelves: only Started stays
            L.recent, L.captions = false, true
            n = covers(1, false)
            L.cover_h = n > 0 and math.floor(avail - total(1, 0, true, false)) or natural
        end
    elseif L.started and has_started then
        -- Room left for more rows of Started, with covers a little smaller if
        -- need be; much smaller (and more of them in a row) rather than
        -- leaving an empty band of more than 3 L under one row
        local band = avail - total(1, natural, true, L.recent)
        local least = band > 3 * sp.L and 0.45 or 0.75
        for rows = 2, parts.max_rows or 2 do
            if #self.started <= (rows - 1) * L.count then break end
            local cover_h = math.min(natural, math.floor((avail - total(rows, 0, true, L.recent)) / covers(rows, L.recent)))
            if cover_h < natural * least then break end
            L.rows, L.cover_h = rows, cover_h
            if band <= 3 * sp.L then least = 0.75 end
        end
    end
    L.cover_h = math.max(Screen:scaleBySize(40), math.min(natural, L.cover_h))
    L.cover_w = math.min(natural_w, math.floor(L.cover_h / Browser.COVER_RATIO))
    L.cover_h = math.floor(L.cover_w * Browser.COVER_RATIO)
    if L.cover_w < natural_w * 0.8 then
        -- Smaller covers: more of them in a row rather than wide gaps between them
        L.count = math.max(L.count, math.min(12, math.floor((w + gap) / (L.cover_w + gap))))
        -- Not more in a row than the rows of Started can fill
        if L.rows > 1 and has_started then L.count = math.min(L.count, math.ceil(#self.started / L.rows)) end
    end
    if L.cover_w < Screen:scaleBySize(90) then
        -- Narrow covers: their titles a little smaller, so words are not cut
        -- (what that frees goes between the sections or the rows)
        L.caption_face = Font:getFace("cfont", 12)
        L.meta_face = Font:getFace("smallinfofont", 12)
        L.caption_h = linesHeight(L.caption_face, 2, L.cover_w)
        L.meta_h = TextWidget:new{ text = "0 %", face = L.meta_face }:getSize().h
        started_extra = 2 * L.space + L.bar_h + L.caption_h + L.meta_h
    end
    L.cell_w = L.cover_w
    L.started_row_h = L.cover_h + started_extra
    L.recent_row_h = L.cover_h + recentExtra(L.captions)
    -- What is left of the height goes between the sections, evenly, or
    -- between the rows of a column that only has Started
    local used, blocks = total(L.rows, L.cover_h, L.captions, L.recent)
    L.used = used
    local spare = avail - used
    if spare > 0 then
        if blocks > 1 then
            -- The space under the last block (L over the tabs) gets its
            -- share too, so it is as wide as the gaps between the sections
            local share = math.floor(spare / blocks)
            L.gap_l = sp.L + share
            L.bottom = share
        elseif L.started and has_started and L.rows > 1 then
            L.gap_row = sp.G + math.floor(spare / (L.rows - 1))
        end
    end
    return L
end

-- The parts laid out by `L`, one under the other
function Home:column(w, L)
    local column = VerticalGroup:new{ align = "left" }
    local function add(widget)
        if #column > 0 then table.insert(column, VerticalSpan:new{ width = L.gap_l }) end
        table.insert(column, widget)
    end
    if L.card then add(L.card) end
    if L.started then
        local shown = math.min(#self.started, L.rows * L.count)
        add(self:sectionHeader(w, _("Started"), #self.started > shown and _("See all") or nil,
            function() self:showStarted() end))
        if #self.started == 0 then
            table.insert(column, self:emptyLine(w, L.empty_h, _("Books you start, here or on your other devices, will show here.")))
        end
        for r = 1, L.rows do
            local items = {}
            for i = (r - 1) * L.count + 1, math.min(r * L.count, shown) do
                local record = self.started[i]
                items[#items + 1] = self:startedItem(record, L)
                if record.on_device then table.insert(self.shown_local, record) end
                if record.entry and record.entry.thumbnail then table.insert(self.shown_entries, record.entry) end
            end
            if #items > 0 then
                if r > 1 then table.insert(column, VerticalSpan:new{ width = L.gap_row }) end
                table.insert(column, self:shelf(items, L, "started"))
            end
        end
    end
    if L.recent then
        add(self:sectionHeader(w, _("Recently added to the library"), self.plugin:ready() and _("See all") or nil, function()
            self:openLibrary(false, self.recent_url or (self.plugin.client.server .. "/opds/books"), _("Recently added"))
        end))
        if #self.recent_shown == 0 then
            table.insert(column, self:emptyLine(w, L.empty_h, self.plugin:ready() and _("The library's newest books will show here.")
                or _("Set up your OPDShelf server in the menu to see your library here.")))
        else
            table.insert(column, self:shelf(self:recentItems(L.count), L, "recent"))
            for i = 1, math.min(L.count, #self.recent_shown) do
                if self.recent_shown[i].thumbnail then table.insert(self.shown_entries, self.recent_shown[i]) end
            end
        end
    end
    if (L.bottom or 0) > 0 then table.insert(column, VerticalSpan:new{ width = L.bottom }) end
    return column
end

--[[--
The whole screen: the header and the tiles, the content and the tabs at the
bottom, all between the side margins M; the content takes all the height
between the tiles (G under them) and the tabs (L over them, as between sections).
--]]
function Home:refresh()
    if self.closed then return end
    if self[1] then self[1]:free() end
    local W, H = self.dimen.w, self.dimen.h
    local sp = Home.spacing()
    self.sp = sp
    self.margin = sp.M
    self.gap_h = sp.gap
    local w = W - 2 * sp.M
    self.started = self:startedBooks()
    self.recent_shown = self:recentNotShown()
    local header = self:header(w)
    local tabs = self:tabBar()
    local tabs_h = tabs:getSize().h
    local avail = H - sp.G - header:getSize().h - sp.G - sp.L - tabs_h
    -- What fetchCovers looks for
    self.shown_entries, self.shown_local = {}, {}
    if self.history[1] then self.shown_local[1] = self.history[1] end

    local content
    if W > H then
        -- Landscape: the card and the newest books on the left, Started on the right
        local gap = sp.L
        local left_w = math.floor((w - gap) * 0.42)
        local right_w = w - gap - left_w
        local left = self:layout(left_w, avail, { card = true, recent = true, card_max = math.floor(avail * 0.5) })
        local right = self:layout(right_w, avail, { started = true, max_rows = 3 })
        -- With room under Started, the newest books go there, a full row with
        -- their titles, and the card takes its column with a large cover:
        -- no gap under Started, no squeezed shelf beside it
        if self.history[1] and #self.recent_shown > 0 then
            local both = self:layout(right_w, avail, { started = true, recent = true, captions = true, max_rows = 3 })
            if both.recent and both.captions and both.rows >= right.rows and both.cover_h >= right.cover_h * 0.75 then
                local card = self:layout(left_w, avail, { card = true, tall = true, card_max = avail })
                if card.card_tall then left, right = card, both end
            end
        end
        -- A short left column: the card grows into the room rather than leaving a band under it
        local room = left.gap_l - sp.L + (left.bottom or 0)
        if left.card and not left.card_tall and room > 2 * sp.L then
            local grown = self:layout(left_w, avail, { card = true, recent = left.recent, tall = true,
                card_max = left.card_h + room })
            if grown.card_tall then left = grown end
        end
        local recent = right.recent and right or left
        self.layout_info = { left = left, right = right, count = right.count, rows = right.rows,
            cover_w = right.cover_w, cover_h = right.cover_h, captions = recent.recent and recent.captions,
            recent = left.recent or right.recent, recent_beside = right.recent or nil, avail = avail }
        content = HorizontalGroup:new{
            align = "top",
            self:column(left_w, left),
            HorizontalSpan:new{ width = gap },
            self:column(right_w, right),
        }
    else
        local L = self:layout(w, avail, {
            card = true, started = true, recent = true, max_rows = 2,
            card_max = math.floor(math.min(H * 0.27, avail * 0.4)),
        })
        L.avail = avail
        self.layout_info = L
        content = self:column(w, L)
    end
    content.home_block = "content"

    local body = VerticalGroup:new{
        align = "left",
        VerticalSpan:new{ width = sp.G },
        header,
        VerticalSpan:new{ width = sp.G },
        content,
    }
    self[1] = FrameContainer:new{
        width = W,
        height = H,
        bordersize = 0,
        padding = 0,
        background = Blitbuffer.COLOR_WHITE,
        OverlapGroup:new{
            dimen = self.dimen:copy(),
            HorizontalGroup:new{ HorizontalSpan:new{ width = sp.M }, body },
            VerticalGroup:new{
                align = "left",
                VerticalSpan:new{ width = H - tabs_h },
                tabs,
            },
        },
    }
    UIManager:setDirty(self, "ui")
end

-- ---------------------------------------------------------------------------
-- Started: all of them

local StartedList = InputContainer:extend{
    home = nil,
    covers_fullscreen = true,
}

function StartedList:init()
    self.dimen = Geom:new{ x = 0, y = 0, w = Screen:getWidth(), h = Screen:getHeight() }
    self.page = 1
    if Device:hasKeys() then
        self.key_events.Close = { { Input.group.Back } }
        self.key_events.NextPage = { { Input.group.PgFwd } }
        self.key_events.PrevPage = { { Input.group.PgBack } }
    end
    if Device:isTouchDevice() then
        self.ges_events.Swipe = { GestureRange:new{ ges = "swipe", range = self.dimen } }
    end
    self[1] = FrameContainer:new{ width = self.dimen.w, height = self.dimen.h, bordersize = 0, padding = 0,
        background = Blitbuffer.COLOR_WHITE, VerticalSpan:new{ width = 0 } }
end

function StartedList:row(record, w, h)
    local home = self.home
    local sp = Home.spacing()
    local inner_h = h - 2 * sp.G - Size.line.thin
    local cover_w = math.floor(inner_h / Browser.COVER_RATIO)
    -- Between the screen's margins, as on the home screen
    local text_w = w - 2 * sp.M - sp.gap - cover_w
    local face = Font:getFace("cfont", 18)
    local small = Font:getFace("cfont", 15)
    local lines = VerticalGroup:new{
        align = "left",
        TextBoxWidget:new{ text = record.title, face = face, bold = true, width = text_w,
            height = linesHeight(face, 2, text_w), height_adjust = true, height_overflow_show_ellipsis = true },
    }
    local function add(text, f, color)
        if not text or text == "" then return end
        table.insert(lines, VerticalSpan:new{ width = sp.S })
        table.insert(lines, TextWidget:new{ text = text, face = f or small, fgcolor = color, max_width = text_w })
    end
    add(table.concat(record.authors or {}, ", "), small, GRAY)
    add(home:progressText(record, true) .. " · " .. (record.on_device and _("on this device") or _("in the library")))
    table.insert(lines, VerticalSpan:new{ width = sp.S })
    table.insert(lines, bar(math.min(text_w, Screen:scaleBySize(160)), Screen:scaleBySize(4), record.percent))
    return VerticalGroup:new{
        align = "left",
        Item:new{
            width = w, height = h - Size.line.thin,
            callback = function() home:openRecord(record) end,
            hold_callback = function() home:recordActions(record) end,
            LeftContainer:new{
                dimen = Geom:new{ w = w, h = h - Size.line.thin },
                HorizontalGroup:new{
                    align = "center",
                    HorizontalSpan:new{ width = sp.M },
                    home:cover(record.title, home:recordCover(record), cover_w, inner_h, Home.badgeFor(record.on_device, record.finished)),
                    HorizontalSpan:new{ width = sp.gap },
                    lines,
                },
            },
        },
        LineWidget:new{ background = Blitbuffer.COLOR_LIGHT_GRAY, dimen = Geom:new{ w = w, h = Size.line.thin } },
    }
end

function StartedList:pages()
    return math.max(1, math.ceil(#self.books / self.per_page))
end

function StartedList:refresh()
    if self.closed then return end
    if self[1] then self[1]:free() end
    local W, H = self.dimen.w, self.dimen.h
    local title_bar = TitleBar:new{
        width = W, fullscreen = true, with_bottom_line = true,
        title = _("Started"),
        subtitle = #self.books == 1 and _("1 book") or T(_("%1 books"), #self.books),
        close_callback = function() self:onClose() end,
        show_parent = self,
    }
    local footer_h = Screen:scaleBySize(48)
    local content_h = H - title_bar:getHeight() - footer_h
    self.per_page = math.max(1, math.floor(content_h / Screen:scaleBySize(110)))
    local row_h = math.floor(content_h / self.per_page)
    self.page = math.max(1, math.min(self.page, self:pages()))
    local content = VerticalGroup:new{ align = "left" }
    local first = (self.page - 1) * self.per_page + 1
    for i = first, math.min(first + self.per_page - 1, #self.books) do
        table.insert(content, self:row(self.books[i], W, row_h))
    end
    local prev = Button:new{ icon = "chevron.left", bordersize = 0, show_parent = self, callback = function() self:onPrevPage() end }
    local nxt = Button:new{ icon = "chevron.right", bordersize = 0, show_parent = self, callback = function() self:onNextPage() end }
    if self.page <= 1 then prev:disable() end
    if self.page >= self:pages() then nxt:disable() end
    local spacer = HorizontalSpan:new{ width = Screen:scaleBySize(24) }
    local nav = HorizontalGroup:new{ align = "center", prev, spacer,
        TextWidget:new{ text = T(_("Page %1 of %2"), self.page, self:pages()), face = Font:getFace("pgfont") },
        spacer, nxt }
    self[1] = FrameContainer:new{
        width = W, height = H, bordersize = 0, padding = 0, background = Blitbuffer.COLOR_WHITE,
        OverlapGroup:new{
            dimen = self.dimen:copy(),
            VerticalGroup:new{ align = "left", title_bar, content },
            VerticalGroup:new{
                align = "left",
                VerticalSpan:new{ width = H - footer_h },
                CenterContainer:new{ dimen = Geom:new{ w = W, h = footer_h }, nav },
            },
        },
    }
    UIManager:setDirty(self, "ui")
end

function StartedList:onNextPage()
    if self.page < self:pages() then self.page = self.page + 1 self:refresh() end
    return true
end

function StartedList:onPrevPage()
    if self.page > 1 then self.page = self.page - 1 self:refresh() end
    return true
end

function StartedList:onSwipe(_arg, ges)
    if ges.direction == "west" then return self:onNextPage() end
    if ges.direction == "east" then return self:onPrevPage() end
end

function StartedList:onClose()
    if self.closed then return true end
    self.closed = true
    UIManager:close(self, "flashui")
    if self.home and self.home.list == self then self.home.list = nil end
    return true
end

function Home:showStarted()
    -- Every started book, the one in the Continue card too
    local list = StartedList:new{ home = self, books = self:startedBooks(true) }
    self.list = list
    UIManager:show(list, "flashui")
    list:refresh()
    return list
end

-- ---------------------------------------------------------------------------
-- Actions

function Home:openBook(path)
    self:onClose()
    local ReaderUI = require("apps/reader/readerui")
    ReaderUI:showReader(path)
end

-- A started book: open it when it is here, else its details (to download it)
function Home:openRecord(record)
    if record.on_device and exists(record.path) then
        self:openBook(record.path)
    elseif record.entry then
        self.browser:showDetails(record.entry)
    else
        UIManager:show(InfoMessage:new{ text = _("This book is no longer in the library."), timeout = 3 })
    end
end

-- Holding a started book: the library's menu, or Read for a book on this device
function Home:recordActions(record)
    if record.on_device then return self:localActions(record) end
    if record.entry then return self.browser:bookActions(record.entry) end
end

function Home:localActions(book)
    local dialog
    local function item(text, callback)
        return { { text = text, align = "left", callback = function()
            UIManager:close(dialog)
            callback()
        end } }
    end
    local buttons = { item(_("Read"), function() self:openBook(book.path) end) }
    if book.entry then
        buttons[#buttons + 1] = item(_("Book details"), function() self.browser:showDetails(book.entry) end)
    end
    dialog = ButtonDialog:new(Browser.holdDialog(book.title, buttons))
    UIManager:show(dialog)
    return dialog
end

function Home:openLibrary(search, url, title)
    self.plugin:showLibrary(search, url, title)
end

--- KOReader's own screens, opened from the file browser that is under this
--- one. All but the file browser show over this screen, so closing them comes
--- back here.
function Home:nativeEntries()
    return {
        { id = "files", text = _("File browser") },
        { id = "history", text = _("History"), module = "history", method = "onShowHist", over = true },
        { id = "favorites", text = _("Favorites"), module = "collections", method = "onShowColl", over = true },
        { id = "collections", text = _("Collections"), module = "collections", method = "onShowCollList", over = true },
        { id = "search", text = _("File search"), module = "filesearcher", method = "onShowFileSearch", over = true },
        -- Frontlight, Wi-Fi, device and screen settings
        { id = "menu", text = _("KOReader menu"), module = "menu", method = "onShowMenu", over = true },
    }
end

--[[--
A screen shown over this one that goes to a folder of the file browser
("Show folder" in History or Collections, a folder found by File search)
changes the file browser hidden under this screen: then this one leaves, so
the folder is seen. Only while this screen is open: closing it puts the file
browser's own way back.
--]]
function Home:followFileBrowser(fm)
    local chooser = type(fm) == "table" and fm.file_chooser or nil
    if type(chooser) ~= "table" or self.followed == chooser then return end
    local method = chooser.changeToPath
    if type(method) ~= "function" then return end
    self:unfollowFileBrowser()
    self.followed, self.followed_own = chooser, rawget(chooser, "changeToPath")
    chooser.changeToPath = function(...)
        self:onClose()
        return method(...)
    end
end

function Home:unfollowFileBrowser()
    local chooser = self.followed
    if not chooser then return end
    self.followed = nil
    chooser.changeToPath = self.followed_own
    self.followed_own = nil
end

--[[--
Calls `method` of the file browser's module `module` (nil: just the file
browser). This screen closes first, unless `over` keeps it under what opens.
When this KOReader lacks that module or it fails, the file browser is what
shows. Returns whether the screen asked for opened.
--]]
function Home:callNative(module, method, over)
    local ok, FileManager = pcall(require, "apps/filemanager/filemanager")
    local fm = ok and type(FileManager) == "table" and FileManager.instance or nil
    local target = fm and module and fm[module]
    local fn = type(target) == "table" and target[method] or nil
    if not over or type(fn) ~= "function" then self:onClose() end
    if not module then return true end
    if type(fn) == "function" then
        self:followFileBrowser(fm)
        local done, err = pcall(fn, target)
        if done then return true end
        logger.warn("OPDShelf: cannot open", module, method, err)
        if over then self:onClose() end
    end
    if not fm and ok and type(FileManager) == "table" and FileManager.showFiles then
        pcall(FileManager.showFiles, FileManager)
    end
    return false
end

function Home:openNative(id)
    for _i, entry in ipairs(self:nativeEntries()) do
        if entry.id == id then return self:callNative(entry.module, entry.method, entry.over) end
    end
    return false
end

function Home:showHistory()
    return self:openNative("history")
end

-- The menu of the header and of the last tab: KOReader's screens, then the plugin's own things
function Home:showMenu()
    local dialog
    local function button(text, callback)
        return { text = text, callback = function()
            UIManager:close(dialog)
            callback()
        end }
    end
    local buttons = { { { text = _("KOReader"), enabled = false } } }
    local native = self:nativeEntries()
    for i = 1, #native, 2 do
        local row = {}
        for j = i, math.min(i + 1, #native) do
            local entry = native[j]
            row[#row + 1] = button(entry.text, function() self:openNative(entry.id) end)
        end
        buttons[#buttons + 1] = row
    end
    buttons[#buttons + 1] = { { text = _("OPDShelf"), enabled = false } }
    buttons[#buttons + 1] = {
        button(_("Refresh"), function() self:refreshAll() end),
        button(_("Settings") .. " ›", function() self:showSettings() end),
    }
    buttons[#buttons + 1] = { button(_("Status and updates"), function() self.plugin:showStatus() end) }
    dialog = ButtonDialog:new{
        buttons = buttons,
        width = math.floor(math.min(self.dimen.w, self.dimen.h) * 0.8),
    }
    UIManager:show(dialog)
    return dialog
end

-- The plugin's settings, the same list as Tools › OPDShelf › Settings
function Home:showSettings()
    local items = self.plugin:settingsItems(function(what)
        if what == "account" then self:loadRemote() else self:refreshAll() end
    end)
    return Menu.dialog(_("OPDShelf settings"), items)
end

function Home:onSwipe(_arg, ges)
    if ges.direction == "south" then
        if ges.pos and ges.pos.y and ges.pos.y < self.dimen.h / 8 then
            -- From the top edge, as in the file browser: KOReader's menu
            self:callNative("menu", "onShowMenu", true)
        else
            -- Pull down to refresh, as on phones
            self:refreshAll()
        end
        return true
    end
end

-- A book opened from KOReader's screens shown over this one
function Home:onShowingReader()
    self:onClose()
end

function Home:onClose()
    if self.closed then return true end
    self.closed = true
    if self.clock_task then UIManager:unschedule(self.clock_task) self.clock_task = nil end
    if self.list then self.list:onClose() end
    self:unfollowFileBrowser()
    UIManager:close(self, "flashui")
    if self.plugin then self.plugin.home = nil end
    return true
end

function Home:onCloseWidget()
    -- Covers taken from the books stay cached; nothing else to free
    self.closed = true
    self:unfollowFileBrowser()
end

-- ---------------------------------------------------------------------------
-- Turning the screen

-- The new size, when the screen turned or changed (see Browser.fitScreen)
function Home:onScreenResize(dimen)
    if Browser.fitScreen(self, dimen) then
        self.browser.dimen = self.dimen:copy()
        self:refresh()
        UIManager:setDirty(self, "flashui")
    end
end
Home.onSetDimensions = Home.onScreenResize

-- Turned while this screen is on top: the file browser under it turns it
function Home:onSetRotationMode(rotation)
    return Browser.turnScreen(rotation)
end

-- Whatever turned the screen without saying so, the next paint has the right size
function Home:paintTo(bb, x, y)
    if not self.closed and Browser.fitScreen(self) then
        self.browser.dimen = self.dimen:copy()
        self:refresh()
    end
    InputContainer.paintTo(self, bb, x, y)
end

function StartedList:onScreenResize(dimen)
    if Browser.fitScreen(self, dimen) and not self.closed then
        self:refresh()
        UIManager:setDirty(self, "flashui")
    end
end
StartedList.onSetDimensions = StartedList.onScreenResize

function StartedList:onSetRotationMode(rotation)
    return Browser.turnScreen(rotation)
end

function StartedList:paintTo(bb, x, y)
    if not self.closed and self.books and Browser.fitScreen(self) then self:refresh() end
    InputContainer.paintTo(self, bb, x, y)
end

Home.StartedList = StartedList
return Home

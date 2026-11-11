--[[--
OPDShelf's home screen: what to read next, at a glance.

- Continue reading: the last book, its cover, how far it was read here and,
  when another device went further, where that one is.
- Recently added on the server, as a shelf of covers.
- The last books opened on this device, with their progress.
- Library, search and the file browser one tap away.

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
local logger = require("logger")
local Browser = require("opdshelf_browser")
local OPDS = require("opdshelf_opds")
local Sync = require("opdshelf_sync")
local T = require("ffi/util").template
local _ = require("opdshelf_i18n")
local Input = Device.input
local Screen = Device.screen

local COVER_RATIO = Browser.COVER_RATIO
local coverWidget = Browser.coverWidget
local Item = Browser.Item

-- Covers of books on this device, kept while KOReader runs (they take a while to extract)
local local_covers = {}

local function exists(path)
    return path and lfs.attributes(path, "mode") == "file"
end

local function fileTitle(path)
    return (path:match("([^/]+)$") or path):gsub("%.[^.]+$", "")
end

-- What KOReader knows of a book on this device
local function localBook(path)
    local book = { path = path, title = fileTitle(path), authors = {} }
    local ok = pcall(function()
        local DocSettings = require("docsettings")
        if not DocSettings:hasSidecarFile(path) then return end
        local settings = DocSettings:open(path)
        local props = settings:readSetting("doc_props") or {}
        if props.title and props.title ~= "" then book.title = props.title end
        if props.authors and props.authors ~= "" then
            for author in tostring(props.authors):gmatch("[^\n]+") do book.authors[#book.authors + 1] = author end
        end
        book.percent = settings:readSetting("percent_finished")
        local summary = settings:readSetting("summary") or {}
        book.finished = summary.status == "complete"
        book.document = settings:readSetting("partial_md5_checksum")
    end)
    if not ok then logger.dbg("OPDShelf: cannot read the settings of", path) end
    return book
end

local Home = InputContainer:extend{
    plugin = nil,
    covers_fullscreen = true, -- hint for UIManager:_repaint()
}

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
    self.recent = self.plugin.state:readSetting("home_recent") or {}
    self.server_count = self.plugin.state:readSetting("home_count")
    self:loadLocal()
    self[1] = FrameContainer:new{ width = self.dimen.w, height = self.dimen.h, bordersize = 0, padding = 0,
        background = Blitbuffer.COLOR_WHITE, VerticalSpan:new{ width = 0 } }
end

function Home:show()
    UIManager:show(self)
    self:refresh()
    UIManager:scheduleIn(0.3, function() self:loadRemote() end)
    self:scheduleClock()
end

-- The clock in the status bar changes on the minute, repainting only itself
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

-- Clock and date on the left; Wi-Fi and battery on the right
function Home:statusBar()
    local w = self.dimen.w - 2 * self.margin
    local face = Font:getFace("smallinfofont", 15)
    self.clock_widget = TextWidget:new{ text = self:clockText(), face = face, max_width = math.floor(w * 0.6) }
    local right = HorizontalGroup:new{ align = "center" }
    local h = self.clock_widget:getSize().h
    local wifi = false
    pcall(function() wifi = NetworkMgr:isWifiOn() end)
    if wifi then
        table.insert(right, IconWidget:new{ icon = "wifi", width = h, height = h })
        table.insert(right, HorizontalSpan:new{ width = Size.padding.default * 2 })
    end
    local battery
    pcall(function()
        local powerd = Device:getPowerDevice()
        if Device:hasBattery() then
            battery = T(_("%1 %"), powerd:getCapacity()) .. (powerd:isCharging() and " +" or "")
        end
    end)
    if battery then table.insert(right, TextWidget:new{ text = battery, face = face }) end
    local bar_h = h + 2 * Size.padding.small
    self.status_bar = OverlapGroup:new{
        dimen = Geom:new{ w = w, h = bar_h },
        LeftContainer:new{ dimen = Geom:new{ w = w, h = bar_h }, self.clock_widget },
        RightContainer:new{ dimen = Geom:new{ w = w, h = bar_h }, right },
    }
    return self.status_bar
end

-- The tabs at the bottom, as in a phone app
function Home:tabBar()
    local tabs = {
        { "home", _("Home"), function() self:refreshAll() end, true },
        { "book.opened", _("Library"), function() self:openLibrary() end },
        { "appbar.search", _("Search"), function() self:openLibrary(true) end },
        { "appbar.filebrowser", _("Files"), function() self:onClose() end },
        { "appbar.settings", _("Settings"), function() self:showMenu() end },
    }
    local tab_w = math.floor(self.dimen.w / #tabs)
    local icon = Screen:scaleBySize(30)
    local face = Font:getFace("smallinfofont", 13)
    local tab_h = icon + face.size + 4 * Size.padding.default
    local row = HorizontalGroup:new{ align = "top" }
    for _i, tab in ipairs(tabs) do
        local active = tab[4]
        local column = VerticalGroup:new{
            align = "center",
            IconWidget:new{ icon = tab[1], width = icon, height = icon },
            VerticalSpan:new{ width = Size.padding.small },
            TextWidget:new{ text = tab[2], face = face, bold = active, max_width = tab_w - Size.padding.small },
        }
        table.insert(row, Item:new{
            width = tab_w,
            height = tab_h,
            callback = tab[3],
            OverlapGroup:new{
                dimen = Geom:new{ w = tab_w, h = tab_h },
                CenterContainer:new{ dimen = Geom:new{ w = tab_w, h = tab_h }, column },
                -- The tab shown is marked with a bar on top
                active and CenterContainer:new{
                    dimen = Geom:new{ w = tab_w, h = Screen:scaleBySize(3) },
                    LineWidget:new{ background = Blitbuffer.COLOR_BLACK, dimen = Geom:new{ w = math.floor(tab_w * 0.6), h = Screen:scaleBySize(3) } },
                } or VerticalSpan:new{ width = 0 },
            },
        })
    end
    return VerticalGroup:new{
        align = "center",
        LineWidget:new{ background = Blitbuffer.COLOR_LIGHT_GRAY, dimen = Geom:new{ w = self.dimen.w, h = Size.line.thin } },
        row,
    }
end

function Home:refreshAll()
    self:loadLocal()
    self:refresh()
    self:loadRemote()
end

-- The last books read here, newest first
function Home:loadLocal()
    self.history = {}
    local ok, ReadHistory = pcall(require, "readhistory")
    if not ok then return end
    if ReadHistory.reload then pcall(ReadHistory.reload, ReadHistory) end
    for _i, item in ipairs(ReadHistory.hist or {}) do
        if #self.history >= 7 then break end
        if exists(item.file) then
            local book = localBook(item.file)
            book.time = item.time
            self.history[#self.history + 1] = book
        end
    end
end

--- The server's part: its recent books, their covers and where other devices are.
function Home:loadRemote()
    if self.closed or not self.plugin:ready() or not NetworkMgr:isOnline() then
        self.offline = not NetworkMgr:isOnline()
        if not self.closed then self:refresh() end
        return
    end
    local client = self.plugin.client
    local code, body = client:fetch(client.server .. "/")
    local root = code == 200 and OPDS.parse(body, client.server .. "/")
    if root then
        self.connected = true
        self.browser.root = root
        for _i, entry in ipairs(root.entries) do
            local count = entry.summary and tonumber(entry.summary:match("^(%d+)"))
            if count then
                self.server_count = count
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
                if entry.is_book and #self.recent < 12 then self.recent[#self.recent + 1] = entry end
            end
            self.plugin.state:saveSetting("home_recent", self.recent)
            self.plugin.state:saveSetting("home_count", self.server_count)
            self.plugin.state:flush()
        end
    else
        self.connected = false
        self.server_error = code == 401 and _("The server did not accept the username or password.") or nil
    end
    -- Where the book to continue was left on other devices
    local book = self.history[1]
    if book and book.document and self.connected then
        local c, progress = client:getProgress(book.document)
        if c == 200 and type(progress) == "table" and type(progress.current) == "table" then
            local current = progress.current
            self.plugin:noteServerTime(progress)
            if current.device_id ~= self.plugin.device_id and (current.percentage or 0) > (book.percent or 0) + 0.001 then
                book.remote = current
            end
        end
    end
    if self.closed then return end
    self:refresh()
    self:fetchCovers()
end

-- Thumbnails of the server shelf, then covers of local books, then one repaint
function Home:fetchCovers()
    UIManager:scheduleIn(0.1, function()
        if self.closed then return end
        local got = 0
        local shown = self.shelf_count or 4
        for i = 1, math.min(shown, #self.recent) do
            local entry = self.recent[i]
            local path = self.browser:coverPath(entry.thumbnail)
            if path and not exists(path) and not entry.cover_failed then
                local code, data = self.plugin.client:raw("GET", entry.thumbnail, nil, nil, nil, { 3, 6 })
                if code == 200 and #data > 100 then
                    local file = io.open(path, "wb")
                    if file then file:write(data) file:close() got = got + 1 end
                else
                    entry.cover_failed = true
                    if not code then break end
                end
            end
        end
        for i = 1, math.min(shown + 1, #self.history) do
            if self:extractCover(self.history[i]) then got = got + 1 end
        end
        if got > 0 and not self.closed then self:refresh() end
    end)
end

-- The cover of a book on this device: CoverBrowser's cache, the one saved when
-- it was downloaded from the library, or taken from the book itself (once)
function Home:localCover(book)
    local cached = local_covers[book.path]
    if cached then return cached end
    local saved = (self.plugin.state:readSetting("covers") or {})[book.path]
    if exists(saved) then return saved end
    local ok, BookInfoManager = pcall(require, "bookinfomanager")
    if ok and type(BookInfoManager) == "table" and BookInfoManager.getBookInfo then
        local okinfo, info = pcall(BookInfoManager.getBookInfo, BookInfoManager, book.path, true)
        if okinfo and info and info.cover_bb then
            local_covers[book.path] = { bb = info.cover_bb }
            return local_covers[book.path]
        end
    end
    return nil
end

function Home:extractCover(book)
    if self:localCover(book) or book.no_cover then return false end
    local ok, bb = pcall(function()
        local FileManagerBookInfo = require("apps/filemanager/filemanagerbookinfo")
        return FileManagerBookInfo:getCoverImage(nil, book.path)
    end)
    if ok and bb then
        local_covers[book.path] = { bb = bb }
        return true
    end
    book.no_cover = true
    return false
end

-- A small heading with an optional link on the right
function Home:sectionHeader(text, link, callback)
    local w = self.dimen.w - 2 * self.margin
    local label = TextWidget:new{
        text = text,
        face = Font:getFace("smallinfofontbold", 16),
        max_width = math.floor(w * 0.6),
    }
    local group = OverlapGroup:new{
        dimen = Geom:new{ w = w, h = label:getSize().h + Size.padding.small },
        LeftContainer:new{ dimen = Geom:new{ w = w, h = label:getSize().h }, label },
    }
    if link then
        table.insert(group, RightContainer:new{
            dimen = Geom:new{ w = w, h = label:getSize().h },
            Button:new{
                text = link .. " ›",
                text_font_face = "smallinfofont",
                text_font_size = 15,
                text_font_bold = false,
                bordersize = 0,
                padding = 0,
                padding_h = 0,
                margin = 0,
                callback = callback,
                show_parent = self,
            },
        })
    end
    return group
end

-- Continue reading: the big card at the top
function Home:hero(height)
    local w = self.dimen.w - 2 * self.margin
    local book = self.history[1]
    if not book then
        return CenterContainer:new{
            dimen = Geom:new{ w = w, h = height },
            VerticalGroup:new{
                align = "center",
                TextWidget:new{ text = _("Nothing read yet"), face = Font:getFace("tfont", 22) },
                VerticalSpan:new{ width = Size.padding.large },
                TextBoxWidget:new{
                    text = _("Find a book in your OPDShelf library and download it to start."),
                    face = Font:getFace("cfont", 17), width = math.floor(w * 0.8), alignment = "center",
                },
                VerticalSpan:new{ width = Size.padding.large * 2 },
                Button:new{
                    text = _("Open the library"), width = math.floor(w * 0.6), show_parent = self,
                    callback = function() self:openLibrary() end,
                },
            },
        }
    end
    local padding = Size.padding.large
    local cover_h = height - 2 * padding
    local cover_w = math.floor(cover_h / COVER_RATIO)
    local text_w = w - cover_w - 3 * padding
    local info = VerticalGroup:new{ align = "left" }
    local function add(widget, space)
        if space then table.insert(info, VerticalSpan:new{ width = space }) end
        table.insert(info, widget)
    end
    add(TextWidget:new{ text = _("CONTINUE READING"), face = Font:getFace("smallinfofont", 13), fgcolor = Blitbuffer.COLOR_GRAY_5 })
    add(TextBoxWidget:new{
        text = book.title, face = Font:getFace("tfont", 24), bold = true, width = text_w,
        height = math.floor(cover_h * 0.42), height_adjust = true, height_overflow_show_ellipsis = true,
    }, Size.padding.small)
    if #book.authors > 0 then
        add(TextWidget:new{ text = table.concat(book.authors, ", "), face = Font:getFace("cfont", 18), max_width = text_w }, Size.padding.small)
    end
    local percent = book.percent or 0
    add(ProgressWidget:new{
        width = text_w, height = Screen:scaleBySize(8), percentage = percent,
        margin_h = 0, margin_v = 0, radius = Screen:scaleBySize(4), bordersize = Size.border.thin,
        fillcolor = Blitbuffer.COLOR_BLACK,
    }, Size.padding.large * 2)
    local status = book.finished and _("Finished") or T(_("%1 % read"), math.floor(percent * 100 + 0.5))
    if book.time then
        status = status .. " · " .. Sync.ago(book.time, os.time(), _, T)
    end
    add(TextWidget:new{ text = status, face = Font:getFace("cfont", 16), max_width = text_w }, Size.padding.default)
    if book.remote then
        add(TextBoxWidget:new{
            text = T(_("Further on %1: %2 % (%3). Open it to continue from there."),
                book.remote.device ~= "" and book.remote.device or _("another device"),
                math.floor((book.remote.percentage or 0) * 100 + 0.5),
                Sync.ago(book.remote.timestamp, self.plugin:serverNow(), _, T)),
            face = Font:getFace("cfont", 16), bold = true, width = text_w,
        }, Size.padding.default)
    end
    local card = HorizontalGroup:new{
        align = "center",
        HorizontalSpan:new{ width = padding },
        coverWidget(book, self:localCover(book), cover_w, cover_h),
        HorizontalSpan:new{ width = padding },
        info,
    }
    return Item:new{
        width = w,
        height = height,
        callback = function() self:openBook(book.path) end,
        FrameContainer:new{
            width = w, height = height, padding = 0, margin = 0,
            bordersize = Size.border.thin, radius = Size.radius.window,
            LeftContainer:new{ dimen = Geom:new{ w = w - 2 * Size.border.thin, h = height - 2 * Size.border.thin }, card },
        },
    }
end

-- A row of covers with their titles. `items` have title, cover and a callback.
function Home:shelf(items, height, empty_text)
    local w = self.dimen.w - 2 * self.margin
    local count = self.shelf_count
    local gap = Size.padding.large
    local cell_w = math.floor((w - (count - 1) * gap) / count)
    local face = Font:getFace("cfont", 14)
    local caption_h = 2 * face.size + Size.padding.small
    local bar_h = Screen:scaleBySize(5)
    local cover_h = math.min(height - caption_h - bar_h - 3 * Size.padding.small, math.floor(cell_w * COVER_RATIO))
    local cover_w = math.min(cell_w, math.floor(cover_h / COVER_RATIO))
    if #items == 0 then
        return CenterContainer:new{
            dimen = Geom:new{ w = w, h = height },
            TextBoxWidget:new{ text = empty_text, face = Font:getFace("cfont", 16), width = math.floor(w * 0.8),
                alignment = "center", fgcolor = Blitbuffer.COLOR_GRAY_5 },
        }
    end
    local row = HorizontalGroup:new{ align = "top" }
    for i = 1, math.min(count, #items) do
        local item = items[i]
        local column = VerticalGroup:new{
            align = "center",
            coverWidget(item, item.cover, cover_w, cover_h),
            VerticalSpan:new{ width = Size.padding.small },
        }
        if item.percent then
            table.insert(column, ProgressWidget:new{
                width = cover_w, height = bar_h, percentage = item.percent,
                margin_h = 0, margin_v = 0, radius = 0, bordersize = 0,
                bgcolor = Blitbuffer.COLOR_LIGHT_GRAY, fillcolor = Blitbuffer.COLOR_BLACK,
            })
            table.insert(column, VerticalSpan:new{ width = Size.padding.small })
        end
        table.insert(column, TextBoxWidget:new{
            text = (item.mark and (item.mark .. " ") or "") .. item.title,
            face = face, width = cell_w, height = caption_h,
            height_overflow_show_ellipsis = true, alignment = "center",
        })
        table.insert(row, Item:new{
            width = cell_w,
            height = height,
            callback = item.callback,
            hold_callback = item.hold_callback,
            CenterContainer:new{ dimen = Geom:new{ w = cell_w, h = height }, column },
        })
        if i < count then table.insert(row, HorizontalSpan:new{ width = gap }) end
    end
    return row
end

function Home:serverItems()
    local items = {}
    for _i, entry in ipairs(self.recent) do
        local _path, downloaded = self.browser:localFile(entry)
        items[#items + 1] = {
            title = entry.title,
            cover = self.browser:cachedCover(entry),
            mark = downloaded and "✓" or nil,
            callback = function() self.browser:showDetails(entry) end,
            hold_callback = function() self.browser:quickDownload(entry) end,
        }
    end
    return items
end

function Home:deviceItems()
    local items = {}
    for i = 2, #self.history do
        local book = self.history[i]
        items[#items + 1] = {
            title = book.title,
            cover = self:localCover(book),
            percent = book.finished and 1 or (book.percent or 0),
            callback = function() self:openBook(book.path) end,
        }
    end
    return items
end

function Home:statusLine()
    if not self.plugin:ready() then
        return _("Not connected: set up the server in the menu")
    elseif self.server_error then
        return self.server_error
    elseif self.offline or self.connected == false then
        return _("Offline: showing what is on this device")
    elseif self.server_count then
        return T(_("%1 books in your library"), self.server_count)
    end
    return _("Connecting…")
end

function Home:refresh()
    if self.closed then return end
    if self[1] then self[1]:free() end
    local landscape = self.dimen.w > self.dimen.h
    self.margin = Size.padding.large * 2
    self.shelf_count = landscape and 6 or 4
    local title_bar = TitleBar:new{
        width = self.dimen.w,
        fullscreen = true,
        with_bottom_line = false,
        title = _("Home"),
        subtitle = self:statusLine(),
        left_icon = "appbar.menu",
        left_icon_tap_callback = function() self:showMenu() end,
        show_parent = self,
    }
    local status_bar = self:statusBar()
    local actions = self:tabBar()
    local actions_h = actions:getSize().h
    local recent_header = self:sectionHeader(_("Recently added to the library"), self.plugin:ready() and _("See all") or nil, function()
        self:openLibrary(false, self.recent_url or (self.plugin.client.server .. "/opds/books"), _("Recently added"))
    end)
    local device_header = self:sectionHeader(_("On this device"), _("History"), function() self:showHistory() end)
    local space = Size.padding.large * 2
    -- Everything between the title and the tabs is shared out, so nothing is left empty
    local content_h = self.dimen.h - status_bar:getSize().h - title_bar:getHeight() - actions_h
        - recent_header:getSize().h - device_header:getSize().h - 3 * space
    -- Shelves as tall as their covers need; the card on top takes the rest
    local gap = Size.padding.large
    local cell_w = math.floor((self.dimen.w - 2 * self.margin - (self.shelf_count - 1) * gap) / self.shelf_count)
    local caption = Font:getFace("cfont", 14).size
    local shelf_natural = math.floor(cell_w * COVER_RATIO) + 2 * caption + Screen:scaleBySize(5) + 5 * Size.padding.small
    local shelf_h = math.min(math.floor(content_h * 0.31), shelf_natural)
    local hero_h = content_h - 2 * shelf_h

    local body = VerticalGroup:new{
        align = "center",
        status_bar,
        title_bar,
        self:hero(hero_h),
        VerticalSpan:new{ width = space },
        recent_header,
        self:shelf(self:serverItems(), shelf_h,
            self.plugin:ready() and _("The library's newest books will show here.") or _("Set up your OPDShelf server in the menu to see your library here.")),
        VerticalSpan:new{ width = space },
        device_header,
        self:shelf(self:deviceItems(), shelf_h, _("Other books you open will show here.")),
    }
    self[1] = FrameContainer:new{
        width = self.dimen.w,
        height = self.dimen.h,
        bordersize = 0,
        padding = 0,
        background = Blitbuffer.COLOR_WHITE,
        OverlapGroup:new{
            dimen = self.dimen:copy(),
            CenterContainer:new{
                dimen = Geom:new{ w = self.dimen.w, h = body:getSize().h },
                body,
            },
            VerticalGroup:new{
                align = "center",
                VerticalSpan:new{ width = self.dimen.h - actions_h },
                actions,
            },
        },
    }
    UIManager:setDirty(self, "ui")
end

function Home:openBook(path)
    self:onClose()
    local ReaderUI = require("apps/reader/readerui")
    ReaderUI:showReader(path)
end

function Home:openLibrary(search, url, title)
    self.plugin:showLibrary(search, url, title)
end

function Home:showHistory()
    self:onClose()
    local FileManager = require("apps/filemanager/filemanager")
    if FileManager.instance and FileManager.instance.history then
        FileManager.instance.history:onShowHist()
    end
end

function Home:showMenu()
    local dialog
    local function item(text, callback)
        return { { text = text, align = "left", callback = function()
            UIManager:close(dialog)
            callback()
        end } }
    end
    local starts = self.plugin:launcherEnabled()
    dialog = ButtonDialog:new{
        title = _("OPDShelf"),
        title_align = "center",
        shrink_unneeded_width = true,
        buttons = {
            item(_("Refresh"), function() self:refreshAll() end),
            item(starts and _("Stop opening at start and after a book") or _("Open at start and after a book"), function()
                self.plugin:setLauncher(not starts)
            end),
            item(_("Server and account"), function()
                self.plugin:editAccount(function() self:loadRemote() end)
            end),
            item(T(_("Download folder: %1"), self.plugin:downloadDir()), function()
                self.plugin:chooseDownloadDir()
            end),
        },
    }
    UIManager:show(dialog)
end

function Home:onSwipe(_arg, ges)
    if ges.direction == "south" then
        -- Pull down to refresh, as on phones
        self:refreshAll()
        return true
    end
end

function Home:onClose()
    if self.closed then return true end
    self.closed = true
    if self.clock_task then UIManager:unschedule(self.clock_task) self.clock_task = nil end
    UIManager:close(self, "flashui")
    if self.plugin then self.plugin.home = nil end
    return true
end

function Home:onCloseWidget()
    -- Covers taken from the books stay cached; nothing else to free
    self.closed = true
end

return Home

--[[--
The OPDShelf library inside KOReader: browse the server's OPDS catalog with
covers, as a list or a grid, search it, see a book's details and download it.

@module opdshelf_browser
--]]

local Blitbuffer = require("ffi/blitbuffer")
local BottomContainer = require("ui/widget/container/bottomcontainer")
local Button = require("ui/widget/button")
local ButtonDialog = require("ui/widget/buttondialog")
local ButtonTable = require("ui/widget/buttontable")
local CenterContainer = require("ui/widget/container/centercontainer")
local ConfirmBox = require("ui/widget/confirmbox")
local DataStorage = require("datastorage")
local Device = require("device")
local Font = require("ui/font")
local FrameContainer = require("ui/widget/container/framecontainer")
local Geom = require("ui/geometry")
local GestureRange = require("ui/gesturerange")
local HorizontalGroup = require("ui/widget/horizontalgroup")
local HorizontalSpan = require("ui/widget/horizontalspan")
local ImageWidget = require("ui/widget/imagewidget")
local InfoMessage = require("ui/widget/infomessage")
local InputContainer = require("ui/widget/container/inputcontainer")
local InputDialog = require("ui/widget/inputdialog")
local LeftContainer = require("ui/widget/container/leftcontainer")
local LineWidget = require("ui/widget/linewidget")
local NetworkMgr = require("ui/network/manager")
local OverlapGroup = require("ui/widget/overlapgroup")
local ProgressWidget = require("ui/widget/progresswidget")
local ScrollTextWidget = require("ui/widget/scrolltextwidget")
local Size = require("ui/size")
local TextBoxWidget = require("ui/widget/textboxwidget")
local TextWidget = require("ui/widget/textwidget")
local TitleBar = require("ui/widget/titlebar")
local UIManager = require("ui/uimanager")
local VerticalGroup = require("ui/widget/verticalgroup")
local VerticalSpan = require("ui/widget/verticalspan")
local lfs = require("libs/libkoreader-lfs")
local logger = require("logger")
local md5 = require("ffi/sha2").md5
local util = require("util")
local OPDS = require("opdshelf_opds")
local T = require("ffi/util").template
local _ = require("opdshelf_i18n")
local Input = Device.input
local Screen = Device.screen

local COVER_RATIO = 1.5 -- height / width of a book cover

local function formatSize(bytes)
    if not bytes then return nil end
    if bytes >= 1024 * 1024 then
        return string.format("%.1f MB", bytes / 1024 / 1024)
    end
    return string.format("%d KB", math.max(1, math.floor(bytes / 1024)))
end

local function formatName(acquisition)
    local kind = acquisition and acquisition.type or ""
    return ({
        ["application/epub+zip"] = "EPUB",
        ["application/pdf"] = "PDF",
        ["application/x-cbz"] = "CBZ",
        ["application/vnd.comicbook+zip"] = "CBZ",
        ["application/x-cbr"] = "CBR",
        ["application/x-fictionbook+xml"] = "FB2",
        ["application/x-mobipocket-ebook"] = "MOBI",
        ["text/plain"] = "TXT",
    })[kind] or (acquisition and acquisition.title) or kind:match("/(.+)$") or ""
end

local function seriesLine(entry)
    if not entry.series then return nil end
    if entry.series_index and entry.series_index ~= "" then
        return T(_("%1 #%2"), entry.series, entry.series_index)
    end
    return entry.series
end

-- A cover from the cache (a file path, or { bb = BlitBuffer } kept by the caller),
-- or a frame with the title while it is not there
local function coverWidget(entry, path, width, height)
    if type(path) == "table" and path.bb then
        return CenterContainer:new{
            dimen = Geom:new{ w = width, h = height },
            ImageWidget:new{
                image = path.bb,
                image_disposable = false,
                width = width,
                height = height,
                scale_factor = 0,
            },
        }
    elseif path then
        return CenterContainer:new{
            dimen = Geom:new{ w = width, h = height },
            ImageWidget:new{
                file = path,
                width = width,
                height = height,
                scale_factor = 0,
                file_do_cache = false,
            },
        }
    end
    local border = Size.border.thin
    return FrameContainer:new{
        width = width,
        height = height,
        bordersize = border,
        padding = 0,
        margin = 0,
        background = Blitbuffer.COLOR_LIGHT_GRAY,
        CenterContainer:new{
            dimen = Geom:new{ w = width - 2 * border, h = height - 2 * border },
            TextBoxWidget:new{
                text = entry.title,
                face = Font:getFace("cfont", math.max(10, math.floor(width / Screen:scaleBySize(9)))),
                width = width - 2 * border - 2 * Size.padding.small,
                height = height - 2 * border - 2 * Size.padding.small,
                height_overflow_show_ellipsis = true,
                alignment = "center",
                bgcolor = Blitbuffer.COLOR_LIGHT_GRAY,
            },
        },
    }
end

-- One tappable tile of the list or the grid
local Item = InputContainer:extend{
    width = nil,
    height = nil,
    callback = nil,
    hold_callback = nil,
}

function Item:init()
    self.dimen = Geom:new{ x = 0, y = 0, w = self.width, h = self.height }
    -- A frame around the content, so a tap can flash it like KOReader's own menus
    self[1] = FrameContainer:new{
        bordersize = 0, padding = 0, margin = 0,
        background = Blitbuffer.COLOR_WHITE,
        self[1],
    }
    self.ges_events = {
        TapItem = { GestureRange:new{ ges = "tap", range = self.dimen } },
        HoldItem = { GestureRange:new{ ges = "hold", range = self.dimen } },
    }
end

function Item:flash()
    local frame = self[1]
    if not frame.dimen or G_reader_settings:isFalse("flash_ui") then return end
    frame.invert = true
    UIManager:widgetInvert(frame, frame.dimen.x, frame.dimen.y)
    UIManager:setDirty(nil, "fast", frame.dimen)
    UIManager:forceRePaint()
    if UIManager.yieldToEPDC then UIManager:yieldToEPDC() end
    frame.invert = false
    UIManager:widgetInvert(frame, frame.dimen.x, frame.dimen.y)
    UIManager:setDirty(nil, "ui", frame.dimen)
end

function Item:onTapItem()
    self:flash()
    if self.callback then self.callback() end
    return true
end

function Item:onHoldItem()
    if self.hold_callback then
        self:flash()
        self.hold_callback()
    end
    return true
end

local Browser = InputContainer:extend{
    plugin = nil, -- the OPDShelf plugin: client, settings, downloads
    title = _("OPDShelf library"),
    covers_fullscreen = true, -- hint for UIManager:_repaint()
}

function Browser:init()
    self.dimen = Geom:new{ x = 0, y = 0, w = Screen:getWidth(), h = Screen:getHeight() }
    self.view = self.plugin.state:readSetting("library_view") or "list"
    self.stack = {}
    self.cover_dir = DataStorage:getDataDir() .. "/cache/opdshelf-covers"
    util.makePath(self.cover_dir)
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

-- Opens the catalog root, or `url` if given
function Browser:start(url, title)
    UIManager:show(self)
    self:open(url or (self.plugin.client.server .. "/"), title or self.title)
end

function Browser:currentLevel()
    return self.stack[#self.stack]
end

--- Loads a feed and shows it as a new level.
function Browser:open(url, title)
    NetworkMgr:runWhenOnline(function()
        local feed, err = self:load(url)
        if not feed then
            self:showError(err)
            return
        end
        if #self.stack == 0 then
            self.root = feed
        end
        table.insert(self.stack, {
            url = url,
            title = title ~= "" and title or feed.title,
            feed = feed,
            entries = feed.entries,
            next = feed.next,
            page = 1,
        })
        self:refresh()
    end)
end

function Browser:load(url)
    local loading = InfoMessage:new{ text = _("Loading…"), dismissable = false }
    UIManager:show(loading)
    UIManager:forceRePaint()
    local code, body = self.plugin.client:fetch(url)
    UIManager:close(loading)
    if code == 401 then
        return nil, _("The server did not accept the username or password. Check them in OPDShelf → Server and account.")
    elseif code ~= 200 then
        return nil, code and T(_("The server answered %1."), code) or T(_("Cannot reach the server: %1"), tostring(body))
    end
    local feed, err = OPDS.parse(body, url)
    if not feed then
        return nil, T(_("This is not a library feed: %1"), err)
    end
    return feed
end

function Browser:showError(text)
    UIManager:show(InfoMessage:new{ text = text })
    if #self.stack == 0 then
        self:onClose()
    end
end

-- Entries per page and the size of each tile
function Browser:layout(level)
    local has_books = false
    for _i, entry in ipairs(level.entries) do
        if entry.is_book then has_books = true break end
    end
    local grid = self.view == "grid" and has_books
    local content_h = self.dimen.h - self.title_bar:getHeight() - self.footer_height
    if grid then
        local landscape = self.dimen.w > self.dimen.h
        local cols = landscape and 5 or 3
        local rows = landscape and 2 or 3
        return {
            grid = true, cols = cols, rows = rows, per_page = cols * rows,
            w = math.floor(self.dimen.w / cols), h = math.floor(content_h / rows),
        }
    end
    local min_h = Screen:scaleBySize(has_books and 100 or 64)
    local per_page = math.max(1, math.floor(content_h / min_h))
    return { grid = false, per_page = per_page, w = self.dimen.w, h = math.floor(content_h / per_page) }
end

function Browser:coverPath(url)
    if not url then return nil end
    return self.cover_dir .. "/" .. md5(url) .. ".jpg"
end

function Browser:cachedCover(entry)
    local path = self:coverPath(entry.thumbnail)
    if path and lfs.attributes(path, "mode") == "file" then return path end
    return nil
end

--- Where a book is (or would be) downloaded, and whether it is already there.
function Browser:localFile(entry)
    local acquisition = OPDS.bestAcquisition(entry)
    if not acquisition then return nil end
    local path = self.plugin:downloadDir() .. "/" .. OPDS.fileName(entry, acquisition)
    return path, lfs.attributes(path, "mode") == "file", acquisition
end

-- How far a downloaded book was read here, from its KOReader settings
local function readPercent(path)
    local ok, percent = pcall(function()
        local DocSettings = require("docsettings")
        if not DocSettings:hasSidecarFile(path) then return nil end
        return DocSettings:open(path):readSetting("percent_finished")
    end)
    return ok and percent or nil
end

function Browser:statusLine(entry)
    local parts = {}
    local acquisition = OPDS.bestAcquisition(entry)
    if acquisition then
        parts[#parts + 1] = formatName(acquisition)
        parts[#parts + 1] = formatSize(acquisition.length)
    end
    local path, downloaded = self:localFile(entry)
    if downloaded then
        local percent = readPercent(path)
        if percent and percent > 0 then
            parts[#parts + 1] = T(_("✓ read %1 %"), math.floor(percent * 100 + 0.5))
        else
            parts[#parts + 1] = _("✓ downloaded")
        end
    end
    local text = {}
    for _i, p in ipairs(parts) do
        if p and p ~= "" then text[#text + 1] = p end
    end
    return table.concat(text, " · ")
end

function Browser:listItem(entry, w, h)
    local padding = Size.padding.default
    local inner_h = h - 2 * padding - Size.line.thin
    local text_x = padding
    local row = HorizontalGroup:new{ align = "center" }
    if entry.is_book then
        local cover_h = inner_h
        local cover_w = math.floor(cover_h / COVER_RATIO)
        table.insert(row, coverWidget(entry, self:cachedCover(entry), cover_w, cover_h))
        table.insert(row, HorizontalSpan:new{ width = Size.padding.large })
        text_x = text_x + cover_w + Size.padding.large
    end
    local text_w = w - text_x - padding
    local lines = VerticalGroup:new{ align = "left" }
    local title = TextBoxWidget:new{
        text = entry.title,
        face = Font:getFace("cfont", 20),
        bold = true,
        width = text_w,
        height = entry.is_book and math.floor(inner_h * 0.5) or nil,
        height_overflow_show_ellipsis = true,
        height_adjust = true,
    }
    table.insert(lines, title)
    local small = Font:getFace("cfont", 16)
    local function addLine(text, face)
        if text and text ~= "" then
            table.insert(lines, VerticalSpan:new{ width = Size.padding.small })
            table.insert(lines, TextWidget:new{ text = text, face = face or small, max_width = text_w })
        end
    end
    if entry.is_book then
        addLine(table.concat(entry.authors, ", "))
        addLine(seriesLine(entry), Font:getFace("cfont", 15))
        addLine(self:statusLine(entry), Font:getFace("cfont", 14))
        local path, downloaded = self:localFile(entry)
        local percent = downloaded and readPercent(path)
        if percent and percent > 0 then
            table.insert(lines, VerticalSpan:new{ width = Size.padding.small })
            table.insert(lines, ProgressWidget:new{
                width = math.min(text_w, Screen:scaleBySize(160)), height = Screen:scaleBySize(4),
                percentage = percent, margin_h = 0, margin_v = 0, radius = 0, bordersize = 0,
                bgcolor = Blitbuffer.COLOR_LIGHT_GRAY, fillcolor = Blitbuffer.COLOR_BLACK,
            })
        end
    else
        addLine(entry.summary)
    end
    table.insert(row, lines)
    return VerticalGroup:new{
        align = "left",
        Item:new{
            width = w,
            height = h - Size.line.thin,
            callback = function() self:select(entry) end,
            hold_callback = entry.is_book and function() self:quickDownload(entry) end or nil,
            LeftContainer:new{
                dimen = Geom:new{ w = w, h = h - Size.line.thin },
                HorizontalGroup:new{ HorizontalSpan:new{ width = padding }, row },
            },
        },
        LineWidget:new{
            background = Blitbuffer.COLOR_LIGHT_GRAY,
            dimen = Geom:new{ w = w, h = Size.line.thin },
        },
    }
end

function Browser:gridItem(entry, w, h)
    local padding = Size.padding.default
    local caption_face = Font:getFace("cfont", 14)
    local caption_h = 2 * caption_face.size + Size.padding.small
    local status = entry.is_book and self:statusLine(entry):match("✓[^·]*")
    local status_h = status and (caption_face.size + Size.padding.small) or 0
    local cover_h = h - 2 * padding - caption_h - status_h
    local cover_w = math.min(w - 2 * padding, math.floor(cover_h / COVER_RATIO))
    cover_h = math.floor(cover_w * COVER_RATIO)
    local column = VerticalGroup:new{
        align = "center",
        coverWidget(entry, entry.is_book and self:cachedCover(entry) or nil, cover_w, cover_h),
        VerticalSpan:new{ width = Size.padding.small },
        TextBoxWidget:new{
            text = entry.title,
            face = caption_face,
            width = w - 2 * padding,
            height = caption_h,
            height_overflow_show_ellipsis = true,
            alignment = "center",
        },
    }
    if status then
        table.insert(column, TextWidget:new{ text = status, face = caption_face, max_width = w - 2 * padding })
    end
    return Item:new{
        width = w,
        height = h,
        callback = function() self:select(entry) end,
        hold_callback = entry.is_book and function() self:quickDownload(entry) end or nil,
        CenterContainer:new{ dimen = Geom:new{ w = w, h = h }, column },
    }
end

function Browser:pages(level, layout)
    local pages = math.max(1, math.ceil(#level.entries / layout.per_page))
    if level.next then pages = pages + 1 end -- more on the server
    return pages
end

function Browser:refresh()
    local level = self:currentLevel()
    if not level then return end
    if self[1] then self[1]:free() end
    local depth = #self.stack
    self.title_bar = TitleBar:new{
        width = self.dimen.w,
        fullscreen = true,
        with_bottom_line = true,
        title = level.title,
        subtitle = level.next and T(_("%1+ items"), #level.entries) or T(_("%1 items"), #level.entries),
        left_icon = "appbar.menu",
        left_icon_tap_callback = function() self:showMenu() end,
        close_callback = function() self:onClose() end,
        show_parent = self,
    }
    self.footer_height = Screen:scaleBySize(48)
    local layout = self:layout(level)
    -- Fetch the next page of the feed when reaching its end
    local first = (level.page - 1) * layout.per_page + 1
    if level.next and first + layout.per_page - 1 > #level.entries then
        self:loadMore(level)
    end
    local total_pages = self:pages(level, layout)
    level.page = math.min(level.page, total_pages)
    first = (level.page - 1) * layout.per_page + 1

    local content = VerticalGroup:new{ align = "left" }
    if #level.entries == 0 then
        table.insert(content, CenterContainer:new{
            dimen = Geom:new{ w = self.dimen.w, h = layout.h * 2 },
            TextWidget:new{ text = _("Nothing here."), face = Font:getFace("cfont", 20) },
        })
    elseif layout.grid then
        local index = first
        for _r = 1, layout.rows do
            local row = HorizontalGroup:new{ align = "top" }
            for _c = 1, layout.cols do
                local entry = level.entries[index]
                if entry then
                    table.insert(row, self:gridItem(entry, layout.w, layout.h))
                end
                index = index + 1
            end
            table.insert(content, row)
        end
    else
        for i = first, math.min(first + layout.per_page - 1, #level.entries) do
            table.insert(content, self:listItem(level.entries[i], layout.w, layout.h))
        end
    end

    local nav = HorizontalGroup:new{ align = "center" }
    local back = Button:new{
        icon = "back.top", bordersize = 0, show_parent = self,
        callback = function() self:goBack() end,
    }
    if depth <= 1 then back:disable() end
    local prev = Button:new{
        icon = "chevron.left", bordersize = 0, show_parent = self,
        callback = function() self:onPrevPage() end,
    }
    local nxt = Button:new{
        icon = "chevron.right", bordersize = 0, show_parent = self,
        callback = function() self:onNextPage() end,
    }
    if level.page <= 1 then prev:disable() end
    if level.page >= total_pages then nxt:disable() end
    local spacer = HorizontalSpan:new{ width = Screen:scaleBySize(24) }
    table.insert(nav, prev)
    table.insert(nav, spacer)
    table.insert(nav, TextWidget:new{
        text = T(_("Page %1 of %2"), level.page, total_pages),
        face = Font:getFace("pgfont"),
    })
    table.insert(nav, spacer)
    table.insert(nav, nxt)
    local footer = OverlapGroup:new{
        dimen = Geom:new{ w = self.dimen.w, h = self.footer_height },
        LeftContainer:new{
            dimen = Geom:new{ w = self.dimen.w, h = self.footer_height },
            HorizontalGroup:new{ HorizontalSpan:new{ width = Size.padding.default }, back },
        },
        CenterContainer:new{ dimen = Geom:new{ w = self.dimen.w, h = self.footer_height }, nav },
    }

    self[1] = FrameContainer:new{
        width = self.dimen.w,
        height = self.dimen.h,
        bordersize = 0,
        padding = 0,
        background = Blitbuffer.COLOR_WHITE,
        OverlapGroup:new{
            dimen = self.dimen:copy(),
            VerticalGroup:new{ align = "left", self.title_bar, content },
            BottomContainer:new{ dimen = self.dimen:copy(), footer },
        },
    }
    UIManager:setDirty(self, "ui")
    self:fetchCovers(level, first, layout.per_page)
end

-- Appends the next page of the feed to the level
function Browser:loadMore(level)
    local url = level.next
    level.next = nil
    local code, body = self.plugin.client:fetch(url)
    local feed = code == 200 and OPDS.parse(body, url)
    if not feed then
        logger.warn("OPDShelf: cannot load", url, code)
        level.next = url -- try again on the next page turn
        return
    end
    for _i, entry in ipairs(feed.entries) do
        table.insert(level.entries, entry)
    end
    level.next = feed.next
end

--- Downloads the covers of the page shown, then shows them.
function Browser:fetchCovers(level, first, count)
    local missing = {}
    for i = first, math.min(first + count - 1, #level.entries) do
        local entry = level.entries[i]
        if entry.is_book and entry.thumbnail and not self:cachedCover(entry) and not entry.cover_failed then
            missing[#missing + 1] = entry
        end
    end
    if #missing == 0 then return end
    local token = {}
    self.cover_token = token
    UIManager:scheduleIn(0.2, function()
        if self.cover_token ~= token or self.closed then return end
        local fetched = 0
        local function save(entry, data)
            if type(data) ~= "string" or #data < 100 then
                entry.cover_failed = true
                return
            end
            local path = self:coverPath(entry.thumbnail)
            local file = io.open(path, "wb")
            if file then
                file:write(data)
                file:close()
                fetched = fetched + 1
            end
        end
        local ok, HttpAsync = pcall(require, "httpasync")
        if ok and type(HttpAsync) == "table" and HttpAsync.fetch_many then
            -- Newer KOReader: several at once without freezing on a slow one
            HttpAsync.fetch_many(missing, {
                concurrency = 6,
                get_url = function(entry) return entry.thumbnail end,
                fetch = function(url) return HttpAsync.fetch_url(url, 0, self.plugin.client:headers(url)) end,
                on_success = save,
                on_failure = function(entry) entry.cover_failed = true end,
            })
        else
            for _i, entry in ipairs(missing) do
                local code, data = self.plugin.client:raw("GET", entry.thumbnail, nil, nil, nil, { 3, 6 })
                if code == 200 then save(entry, data) else entry.cover_failed = true end
                if not code then break end -- offline: do not wait for each one
            end
        end
        if fetched > 0 and self.cover_token == token and not self.closed then
            self:refresh()
        end
    end)
end

function Browser:select(entry)
    if entry.is_book then
        self:showDetails(entry)
    elseif entry.navigation then
        self:open(entry.navigation, entry.title)
    end
end

function Browser:goBack()
    if #self.stack <= 1 then return end
    table.remove(self.stack)
    self:refresh()
end

function Browser:onNextPage()
    local level = self:currentLevel()
    if not level then return true end
    if level.page < self:pages(level, self:layout(level)) then
        level.page = level.page + 1
        self:refresh()
    end
    return true
end

function Browser:onPrevPage()
    local level = self:currentLevel()
    if level and level.page > 1 then
        level.page = level.page - 1
        self:refresh()
    end
    return true
end

function Browser:onSwipe(_arg, ges)
    local direction = ges.direction
    if direction == "west" then
        return self:onNextPage()
    elseif direction == "east" then
        return self:onPrevPage()
    elseif direction == "south" and #self.stack > 1 then
        self:goBack()
        return true
    end
end

function Browser:onClose()
    self.closed = true
    self.cover_token = nil
    UIManager:close(self, "flashui")
    if self.plugin and self.plugin.browser == self then
        self.plugin.browser = nil
        -- Opening the library again goes back to where it was
        if self.stack[1] and not self.temporary then
            self.plugin.library_session = { stack = self.stack, root = self.root, view = self.view }
        end
    end
    return true
end

-- Shows the library where it was left in this session
function Browser:resume(session)
    self.stack = session.stack
    self.root = session.root
    self.closed = false
    UIManager:show(self)
    self:refresh()
end

function Browser:showMenu()
    local dialog
    local function item(text, callback)
        return { { text = text, align = "left", callback = function()
            UIManager:close(dialog)
            callback()
        end } }
    end
    local buttons = {
        item(_("Library home"), function()
            while #self.stack > 1 do table.remove(self.stack) end
            self:refresh()
        end),
        item(_("Search…"), function() self:search() end),
        item(self.view == "grid" and _("Show as list") or _("Show covers as a grid"), function()
            self.view = self.view == "grid" and "list" or "grid"
            self.plugin.state:saveSetting("library_view", self.view)
            self.plugin.state:flush()
            local level = self:currentLevel()
            if level then level.page = 1 end
            self:refresh()
        end),
        item(T(_("Download folder: %1"), self.plugin:downloadDir()), function()
            self.plugin:chooseDownloadDir(function() self:refresh() end)
        end),
        item(_("Reload"), function()
            local level = table.remove(self.stack)
            if level then self:open(level.url, level.title) end
        end),
    }
    dialog = ButtonDialog:new{ buttons = buttons, shrink_unneeded_width = true }
    UIManager:show(dialog)
end

function Browser:searchUrl()
    local root = self.root or {}
    if root.search then return root.search end
    return self.plugin.client.server .. "/opds/search?q={searchTerms}"
end

function Browser:search()
    local dialog
    dialog = InputDialog:new{
        title = _("Search the library"),
        input_hint = _("Title, author or series"),
        buttons = { {
            { text = _("Cancel"), id = "close", callback = function() UIManager:close(dialog) end },
            { text = _("Search"), is_enter_default = true, callback = function()
                local query = util.trim(dialog:getInputText() or "")
                UIManager:close(dialog)
                if query == "" then return end
                local url = self:searchUrl():gsub("{searchTerms}", (util.urlEncode(query):gsub("%%", "%%%%")))
                self:open(url, T(_("Search: %1"), query))
            end },
        } },
    }
    UIManager:show(dialog)
    dialog:onShowKeyboard()
end

-- Downloads a book. `after(path)` runs once it is on the device.
function Browser:download(entry, after)
    local path, downloaded, acquisition = self:localFile(entry)
    if not acquisition then
        UIManager:show(InfoMessage:new{ text = _("This book has no download.") })
        return
    end
    local function go()
        NetworkMgr:runWhenOnline(function()
            local size = formatSize(acquisition.length)
            local message = InfoMessage:new{
                text = size and T(_("Downloading %1 (%2)…"), entry.title, size) or T(_("Downloading %1…"), entry.title),
                dismissable = false,
            }
            UIManager:show(message)
            UIManager:forceRePaint()
            util.makePath(self.plugin:downloadDir())
            local ok, err = self.plugin.client:download(acquisition.href, path)
            UIManager:close(message)
            if not ok then
                UIManager:show(InfoMessage:new{ text = T(_("Could not download the book: %1"), tostring(err)) })
                return
            end
            -- Keep its cover, so the home screen shows it without opening the book
            local cover = self:cachedCover(entry)
            if cover then
                local covers = self.plugin.state:readSetting("covers") or {}
                covers[path] = cover
                self.plugin.state:saveSetting("covers", covers)
                self.plugin.state:flush()
            end
            if not self.closed and self.stack[1] then self:refresh() end
            if self.on_download then self.on_download(path) end
            if after then after(path) end
        end)
    end
    if downloaded then
        UIManager:show(ConfirmBox:new{
            text = T(_("%1 is already on this device."), entry.title),
            ok_text = _("Download again"),
            ok_callback = go,
            other_buttons = { { {
                text = _("Read"),
                callback = function() self:openBook(path) end,
            } } },
        })
        return
    end
    go()
end

-- Long press: download without opening the details
function Browser:quickDownload(entry)
    self:download(entry, function(path)
        UIManager:show(ConfirmBox:new{
            text = T(_("%1 downloaded."), entry.title),
            ok_text = _("Read now"),
            cancel_text = _("Later"),
            ok_callback = function() self:openBook(path) end,
        })
    end)
end

function Browser:openBook(path)
    if self.on_open_book then self.on_open_book(path) end
    if self.stack[1] then self:onClose() end
    if self.plugin and self.plugin.home then self.plugin.home:onClose() end
    local ReaderUI = require("apps/reader/readerui")
    ReaderUI:showReader(path)
end

--- A book's details: big cover, metadata, description and buttons.
function Browser:showDetails(entry)
    local Details = InputContainer:extend{ covers_fullscreen = true }
    local details = Details:new{}
    local w, h = self.dimen.w, self.dimen.h
    details.dimen = Geom:new{ x = 0, y = 0, w = w, h = h }
    local padding = Size.padding.large
    local close = function()
        UIManager:close(details, "flashui")
    end
    details.onClose = function() close() return true end
    if Device:hasKeys() then
        details.key_events = { Close = { { Input.group.Back } } }
    end

    local title_bar = TitleBar:new{
        width = w,
        fullscreen = true,
        with_bottom_line = true,
        title = entry.title,
        subtitle = table.concat(entry.authors, ", "),
        close_callback = close,
        show_parent = details,
    }
    local cover_w = math.floor(w * 0.38)
    local cover_h = math.floor(cover_w * COVER_RATIO)
    local cover_path = self:cachedCover(entry)
    -- The full size cover looks better here when it can be had quickly
    if entry.image and entry.image ~= entry.thumbnail and NetworkMgr:isOnline() then
        local big = self:coverPath(entry.image)
        if lfs.attributes(big, "mode") ~= "file" then
            local code, data = self.plugin.client:raw("GET", entry.image, nil, nil, nil, { 3, 8 })
            if code == 200 and #data > 100 then
                local file = io.open(big, "wb")
                if file then file:write(data) file:close() end
            end
        end
        if lfs.attributes(big, "mode") == "file" then cover_path = big end
    end

    local info_w = w - cover_w - 3 * padding
    local info = VerticalGroup:new{ align = "left" }
    local function line(label, value)
        if not value or value == "" then return end
        table.insert(info, TextBoxWidget:new{
            text = label and (label .. " " .. value) or value,
            face = Font:getFace("cfont", 17),
            width = info_w,
        })
        table.insert(info, VerticalSpan:new{ width = Size.padding.default })
    end
    line(nil, seriesLine(entry))
    line(_("Language:"), entry.language)
    line(_("Publisher:"), entry.publisher)
    line(_("Published:"), entry.issued and entry.issued:sub(1, 10))
    if #entry.categories > 0 then line(_("Subjects:"), table.concat(entry.categories, ", ")) end
    local path, downloaded = self:localFile(entry)
    local status = self:statusLine(entry)
    if status ~= "" then
        table.insert(info, VerticalSpan:new{ width = Size.padding.default })
        table.insert(info, TextBoxWidget:new{
            text = status, face = Font:getFace("cfont", 17), bold = downloaded, width = info_w,
        })
        local percent = downloaded and readPercent(path)
        if percent and percent > 0 then
            table.insert(info, VerticalSpan:new{ width = Size.padding.default })
            table.insert(info, ProgressWidget:new{
                width = info_w, height = Screen:scaleBySize(6), percentage = percent,
                margin_h = 0, margin_v = 0, radius = 0, bordersize = 0,
                bgcolor = Blitbuffer.COLOR_LIGHT_GRAY, fillcolor = Blitbuffer.COLOR_BLACK,
            })
        end
    end

    local buttons = { {
        {
            text = downloaded and _("Download again") or _("Download only"),
            font_bold = false,
            callback = function()
                close()
                self:download(entry)
            end,
        },
        {
            text = downloaded and _("Read") or _("Download and read"),
            font_bold = true,
            callback = function()
                close()
                if downloaded then
                    self:openBook(path)
                else
                    self:download(entry, function(p) self:openBook(p) end)
                end
            end,
        },
    } }
    local button_table = ButtonTable:new{ width = w - 2 * padding, buttons = buttons, zero_sep = true, show_parent = details }
    local top_h = math.max(cover_h, info:getSize().h)
    local description_h = h - title_bar:getHeight() - top_h - button_table:getSize().h - 4 * padding
    local body = VerticalGroup:new{
        align = "left",
        title_bar,
        VerticalSpan:new{ width = padding },
        HorizontalGroup:new{
            align = "top",
            HorizontalSpan:new{ width = padding },
            coverWidget(entry, cover_path, cover_w, cover_h),
            HorizontalSpan:new{ width = padding },
            info,
        },
        VerticalSpan:new{ width = padding },
    }
    if entry.summary and description_h > Screen:scaleBySize(40) then
        table.insert(body, HorizontalGroup:new{
            HorizontalSpan:new{ width = padding },
            ScrollTextWidget:new{
                text = util.htmlToPlainTextIfHtml(entry.summary),
                face = Font:getFace("cfont", 17),
                width = w - 2 * padding,
                height = description_h,
                dialog = details,
            },
        })
    end
    details[1] = FrameContainer:new{
        width = w,
        height = h,
        bordersize = 0,
        padding = 0,
        background = Blitbuffer.COLOR_WHITE,
        OverlapGroup:new{
            dimen = Geom:new{ w = w, h = h },
            body,
            BottomContainer:new{
                dimen = Geom:new{ w = w, h = h - padding },
                CenterContainer:new{ dimen = Geom:new{ w = w, h = button_table:getSize().h }, button_table },
            },
        },
    }
    UIManager:show(details)
end

-- Shared with the home screen
Browser.coverWidget = coverWidget
Browser.Item = Item
Browser.readPercent = readPercent
Browser.seriesLine = seriesLine
Browser.COVER_RATIO = COVER_RATIO

return Browser

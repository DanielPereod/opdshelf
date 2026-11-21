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
local Trapper = require("ui/trapper")
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

-- The home screen's spacing (its margin M, steps and gaps), so every screen of the plugin has the same margins
local function spacing()
    return require("opdshelf_home").spacing()
end
-- Whether a file starts as an image KOReader can show (a server's error page saved as a cover does not)
local IMAGE_MAGIC = { "^\255\216\255", "^\137PNG", "^GIF8", "^RIFF....WEBP", "^BM" }
local function imageData(data)
    if type(data) ~= "string" or #data < 100 then return false end
    for _i, magic in ipairs(IMAGE_MAGIC) do
        if data:match(magic) then return true end
    end
    return false
end
local checked_images = {}
local function isImage(path)
    if checked_images[path] ~= nil then return checked_images[path] end
    local file = io.open(path, "rb")
    if not file then return false end
    local head = file:read(100) or ""
    file:close()
    local ok = imageData(head)
    checked_images[path] = ok
    return ok
end

--- Whether a picture is blank: a cover taken from a book can be its empty first page.
local function blankImage(bb)
    local ok, blank = pcall(function()
        local w, h = bb:getWidth(), bb:getHeight()
        if w < 2 or h < 2 then return true end
        for i = 1, 7 do
            for j = 1, 7 do
                local color = bb:getPixel(math.floor(w * i / 8), math.floor(h * j / 8)):getColor8()
                if color.a < 0xF0 then return false end
            end
        end
        return true
    end)
    return ok and blank
end

-- Books of KOReader's history looked at for what is on this device
local MAX_LOCAL = 40

local function formatSize(bytes)
    if not bytes then return nil end
    if bytes >= 1024 * 1024 then
        local text = string.format("%.1f MB", bytes / 1024 / 1024)
        -- "1,6 MB" in Spanish
        if _.spanish() then text = text:gsub("%.", ",") end
        return text
    end
    return string.format("%d KB", math.max(1, math.floor(bytes / 1024)))
end

-- The names of the languages books are most often in; others show their code
local LANGUAGES = {
    es = "Spanish", en = "English", fr = "French", de = "German", it = "Italian", pt = "Portuguese",
    ca = "Catalan", gl = "Galician", eu = "Basque", nl = "Dutch", ru = "Russian", ja = "Japanese",
    zh = "Chinese", la = "Latin", el = "Greek", pl = "Polish", sv = "Swedish", ar = "Arabic",
    spa = "Spanish", eng = "English", fre = "French", fra = "French", ger = "German", deu = "German",
    ita = "Italian", por = "Portuguese", cat = "Catalan",
}

local function languageName(code)
    if not code or code == "" then return nil end
    local base = code:lower():match("^(%a+)")
    local name = base and LANGUAGES[base]
    return name and _(name) or code
end

-- A title cut to `lines` lines of `face` at `width`, with an ellipsis, for
-- places that cannot limit their lines themselves (a dialog's title). The
-- lines are measured as a text box wraps them, words and all.
local function shortTitle(text, face, width, lines)
    text = text or ""
    lines = lines or 2
    local function height(s)
        local box = TextBoxWidget:new{ text = s, face = face, width = width, for_measurement_only = true }
        local h = box:getSize().h
        box:free()
        return h
    end
    local room = height("Ag" .. string.rep("\nAg", lines - 1))
    if height(text) <= room then return text end
    local chars = {}
    for c in text:gmatch("[%z\1-\127\194-\244][\128-\191]*") do chars[#chars + 1] = c end
    local function head(n)
        return (table.concat(chars, "", 1, n):gsub("%s+$", "")) .. "…"
    end
    -- The most characters that still fit, then back to the end of a word
    local lo, hi = 1, #chars
    while lo < hi do
        local mid = math.ceil((lo + hi) / 2)
        if height(head(mid)) <= room then lo = mid else hi = mid - 1 end
    end
    local keep = lo
    for i = lo, math.max(1, lo - 12), -1 do
        if chars[i]:match("^%s$") then keep = i - 1 break end
    end
    return head(math.max(1, keep))
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
-- or a frame with the title while it is not there. A picture narrower than
-- the room is centred in it, or put at its left edge with `left` (in a list,
-- so that every cover starts on the margin); `left == "shrink"` also makes
-- the room as narrow as the picture, so what follows it keeps its distance
local function coverWidget(entry, path, width, height, left)
    if path and (type(path) ~= "table" or path.bb) then
        -- `fit`: as large as fits in w x h; else exactly w x h
        local function image(w, h, fit)
            if type(path) == "table" then
                return ImageWidget:new{ image = path.bb, image_disposable = false, width = w, height = h, scale_factor = fit and 0 or nil }
            end
            return ImageWidget:new{ file = path, width = w, height = h, scale_factor = fit and 0 or nil, file_do_cache = false }
        end
        local picture = image(width, height, true)
        -- KOReader centres a picture in the size asked for: at the left edge,
        -- it is asked for at the size it really takes
        if left and picture.getCurrentWidth then
            picture:getSize() -- renders it, which getCurrentWidth needs
            local w, h = math.floor(picture:getCurrentWidth() or 0), math.floor(picture:getCurrentHeight() or 0)
            if w > 0 and h > 0 and (w < width or h < height) then
                picture = image(w, h)
            end
        end
        local room_w = left == "shrink" and picture:getSize().w or width
        return (left and LeftContainer or CenterContainer):new{ dimen = Geom:new{ w = room_w, h = height }, picture }
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
    UIManager:show(self, "flashui")
    self:open(url or (self.plugin.client.server .. "/"), title or self.title)
end

function Browser:currentLevel()
    return self.stack[#self.stack]
end

--- Loads a feed and shows it as a new level.
function Browser:open(url, title)
    NetworkMgr:runWhenOnline(function()
        if #self.stack == 0 then self:loadProgress() end
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
    -- The server names its sections in English: show them in KOReader's language
    feed.title = _(feed.title)
    for _i, entry in ipairs(feed.entries) do
        if not entry.is_book then
            entry.title = _(entry.title)
            local count, rest = (entry.summary or ""):match("^(%d+) (.+)$")
            if count then entry.summary = T(_("%1 " .. rest), count) end
        end
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
            -- Between the margins, with the same gap between the tiles
            w = math.floor((self.dimen.w - 2 * spacing().M - (cols - 1) * spacing().gap) / cols), h = math.floor(content_h / rows),
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
    if path and lfs.attributes(path, "mode") == "file" then
        if isImage(path) then return path end
        -- Not a picture: the placeholder instead of a blank tile, and it is asked for again
        os.remove(path)
        checked_images[path] = nil
    end
    return nil
end

-- The id of the file at `path` as KOReader syncs it: the one its settings keep or, without
-- them, the partial MD5 of the file. Nil when it cannot be told.
local function fileDocument(path)
    local ok, id = pcall(function()
        local DocSettings = require("docsettings")
        if DocSettings:hasSidecarFile(path) then
            local id = DocSettings:open(path):readSetting("partial_md5_checksum")
            if id then return id end
        end
        return require("util").partialMD5(path)
    end)
    return ok and type(id) == "string" and id or nil
end

--- Where a book is (or would be) downloaded, and whether it is already there.
-- A file of the same name that is another book (two books of the library with one file name,
-- as `Saga A/Book 1.epub` and `Saga B/Book 1.epub`) does not count: the book goes beside it,
-- with its id in the name.
function Browser:localFile(entry)
    local acquisition = OPDS.bestAcquisition(entry)
    if not acquisition then return nil end
    local dir = self.plugin:downloadDir()
    local name = OPDS.fileName(entry, acquisition)
    local path = dir .. "/" .. name
    local present = lfs.attributes(path, "mode") == "file"
    if present and entry.document then
        local here = fileDocument(path)
        if here and here ~= entry.document then
            local suffixed = name:gsub("(%.%w+)$", function(ext) return " [" .. entry.document:sub(1, 8) .. "]" .. ext end)
            if suffixed == name then suffixed = name .. " [" .. entry.document:sub(1, 8) .. "]" end
            path = dir .. "/" .. suffixed
            present = lfs.attributes(path, "mode") == "file"
        end
    end
    return path, present, acquisition
end

-- How far a downloaded book was read here, and whether it is finished, from its KOReader settings
local function readState(path)
    local ok, percent, finished = pcall(function()
        local DocSettings = require("docsettings")
        if not DocSettings:hasSidecarFile(path) then return nil end
        local settings = DocSettings:open(path)
        local summary = settings:readSetting("summary") or {}
        return settings:readSetting("percent_finished"), summary.status == "complete"
    end)
    if not ok then return nil, false end
    return percent, finished or false
end

local function readPercent(path)
    return (readState(path))
end

local function fileTitle(path)
    return ((path:match("([^/]+)$") or path):gsub("%.[^.]+$", ""))
end

--- What KOReader knows of a book on this device: title, authors, how far
--- it was read, whether it is finished, its document id and when its
--- position last changed here (`read_at`, nil when not known).
local function localBook(path)
    local book = { path = path, title = fileTitle(path), authors = {} }
    local ok = pcall(function()
        local DocSettings = require("docsettings")
        if not DocSettings:hasSidecarFile(path) then return end
        local settings = DocSettings:open(path)
        local props = settings:readSetting("doc_props") or {}
        if type(props.title) == "string" and props.title ~= "" then book.title = props.title end
        if type(props.authors) == "string" and props.authors ~= "" then
            for author in props.authors:gmatch("[^\n]+") do book.authors[#book.authors + 1] = author end
        end
        if type(props.description) == "string" and props.description ~= "" then book.description = props.description end
        book.percent = tonumber(settings:readSetting("percent_finished"))
        local summary = settings:readSetting("summary") or {}
        book.finished = summary.status == "complete"
        book.document = settings:readSetting("partial_md5_checksum")
        book.pages = tonumber(settings:readSetting("doc_pages"))
        -- When the position last changed here (KOReader's history says when it was opened)
        local sync = settings:readSetting("opdshelf_sync")
        book.read_at = type(sync) == "table" and tonumber(sync.read_at) or nil
    end)
    if not ok then logger.dbg("OPDShelf: cannot read the settings of", path) end
    return book
end

--- The path in the library of an OPDS entry of OPDShelf ("urn:opdshelf:book:<path>").
local function libraryPath(entry)
    local encoded = type(entry) == "table" and type(entry.id) == "string" and entry.id:match("^urn:opdshelf:book:(.+)$")
    if not encoded then return nil end
    return (encoded:gsub("%%(%x%x)", function(hex) return string.char(tonumber(hex, 16)) end))
end

-- The letters of À to ÿ without their accents, for comparing titles
local PLAIN = "aaaaaaaceeeeiiiidnooooo_ouuuuyps" .. "aaaaaaaceeeeiiiidnooooo_ouuuuypy"

--- Lower case, without accents, spaces or punctuation: one title written two ways compares equal.
local function plainText(text)
    text = type(text) == "string" and text or ""
    text = text:gsub("\195([\128-\191])", function(c)
        local i = c:byte() - 127
        return PLAIN:sub(i, i)
    end)
    return (text:lower():gsub("[%s%p]", ""))
end

local function firstAuthor(authors)
    if type(authors) == "table" then return authors[1] end
    return authors
end

--[[--
Whether two books (anything with `title` and `authors`) are the same one by
their title and first author, written with or without accents, capitals or
punctuation. A side without authors matches on the title alone.
--]]
local function sameBook(a, b)
    local title = plainText(a.title)
    if title == "" or title ~= plainText(b.title) then return false end
    local author_a, author_b = firstAuthor(a.authors), firstAuthor(b.authors)
    if plainText(author_a) == "" or plainText(author_b) == "" then return true end
    -- "Viktor E. Frankl", "Frankl, Viktor": a name in common is enough, the title being the same
    local words = {}
    for word in author_a:gmatch("[^%s%p]+") do
        word = plainText(word)
        if #word >= 3 then words[word] = true end
    end
    for word in author_b:gmatch("[^%s%p]+") do
        if words[plainText(word)] then return true end
    end
    return false
end

-- Whether `b` says where the book is more recently than `a`: a time only
-- guessed from KOReader's history gives way to a known one
local function newer(a, b)
    if b.time == nil then return false end
    if a.time == nil then return true end
    if a.time_guess and not b.time_guess then return true end
    if b.time_guess and not a.time_guess then return false end
    return b.time > a.time
end

-- Two records of one book become one: the newest position wins, finished anywhere is finished
local function absorb(record, other)
    if other.finished then record.finished = true end
    if newer(record, other) then
        record.time, record.time_guess = other.time, other.time_guess
        if (other.percent or 0) > 0 then record.percent = other.percent end
        -- Of two copies here, the one read last is the one to open
        if other.on_device and other.path then record.path = other.path end
    end
    if not record.on_device and other.on_device then record.on_device, record.path = true, other.path end
    record.entry = record.entry or other.entry
    record.lib_path = record.lib_path or other.lib_path
    record.document = record.document or other.document
    record.pages = record.pages or other.pages
    if #(record.authors or {}) == 0 and other.authors then record.authors = other.authors end
end

--[[--
Every book known on this device or on the server, one record each: KOReader's
history here (`locals`, from localBook, with `time` and maybe `time_guess`)
and the server's books (`remote`, records of the home's remoteStarted).

Two records are the same book when their document ids match, when the
library's file is the one here (`localPathFor(entry)` says where it was
downloaded, if it was), or, when one of them has no document id, by their
title and author. Two copies of a book on this device are one book too.
The newest position wins, wherever it was read.

Records: title, authors, percent, finished, time, time_guess, document,
path and on_device (when it is on this device), entry and lib_path (when it
is in the library), pages.
--]]
local function mergeBooks(locals, remote, localPathFor)
    local list, by_document, by_path = {}, {}, {}
    local function find(record)
        local found = (record.document and by_document[record.document]) or (record.path and by_path[record.path])
        if found then return found end
        for _i, other in ipairs(list) do
            if (not record.document or not other.document) and sameBook(record, other) then return other end
        end
    end
    local function add(record, only_merge)
        local found = find(record)
        if found then
            absorb(found, record)
        elseif only_merge then
            return
        else
            list[#list + 1] = record
            found = record
        end
        for _i, r in ipairs({ record, found }) do
            if r.document then by_document[r.document] = found end
            if r.path then by_path[r.path] = found end
        end
    end
    for _i, book in ipairs(locals or {}) do
        add({
            title = book.title, authors = book.authors or {}, percent = book.percent or 0,
            finished = book.finished, time = book.time, time_guess = book.time_guess,
            document = book.document, path = book.path, pages = book.pages, on_device = true,
        })
    end
    for _i, r in ipairs(remote or {}) do
        if type(r) == "table" then
            local path = r.entry and localPathFor and localPathFor(r.entry) or nil
            -- Without a way to open or download it, a book only says how far it was read
            add({
                title = r.title, authors = r.authors or {}, percent = r.percent or 0, finished = r.finished,
                time = r.time, document = r.document, entry = r.entry, lib_path = r.lib_path or libraryPath(r.entry),
                path = path, on_device = path ~= nil,
            }, not (r.entry or path))
        end
    end
    return list
end

--- When a book was published, as the library shows it, or nil when the date is
--- not a real one (Calibre writes 0101-01-01 for "unknown").
local function publishedDate(issued)
    if type(issued) ~= "string" then return nil end
    local year = tonumber(issued:match("%f[%d](%d%d%d%d)%f[%D]"))
    if not year or year < 1000 or year > tonumber(os.date("%Y")) + 1 then return nil end
    if issued:match("^%s*%d%d%d%d%-%d%d%-%d%d") then return issued:match("^%s*(%d%d%d%d%-%d%d%-%d%d)") end
    return issued:match("^%s*(.-)%s*$")
end


--[[--
How far a book was read, as the home screen's Started shelf counts it:
`here` is what this device knows (from localBook), `remote` what the server
knows (a record of the home's remoteStarted). The newest position wins, and a
book read here at a time not known gives way to the server's; finished on
either side is finished. Nil when neither knows of it.
--]]
local function mergeProgress(here, remote)
    if not here and not remote then return nil end
    local percent = here and here.percent or 0
    local finished = here and here.finished or false
    if remote then
        if remote.finished then finished = true end
        local newer = not here or not here.read_at or (remote.time or 0) > here.read_at
        if newer and (remote.percent or 0) > 0 then percent = remote.percent end
    end
    return { percent = percent, finished = finished }
end

-- The server's books, by their path in the library and by document id: the
-- ones loaded with this screen, else those the home screen kept
function Browser:remoteIndex()
    local list = self.remote or self.plugin.state:readSetting("home_started") or {}
    if self.remote_index and self.remote_index.list == list then return self.remote_index end
    local index = { list = list, by_path = {}, by_document = {} }
    for _i, record in ipairs(list) do
        if type(record) == "table" then
            if record.lib_path then index.by_path[record.lib_path] = record end
            if record.document then index.by_document[record.document] = record end
        end
    end
    self.remote_index = index
    return index
end

--- Where the server says every book is: asked once when the library opens.
function Browser:loadProgress()
    if self.remote_loaded or not self.plugin:ready() or not NetworkMgr:isOnline() then return end
    self.remote_loaded = true
    local client = self.plugin.client
    if not client.getLibraryProgress then return end
    local code, body = client:getLibraryProgress()
    if code ~= 200 or type(body) ~= "table" then return end
    if self.plugin.noteServerTime then self.plugin:noteServerTime(body) end
    -- Loaded here, not when this file is: the home screen needs this file first
    local Home = require("opdshelf_home")
    self.remote = Home.remoteStarted(body, nil, self.plugin.server_offset, client.server)
end

--- The books opened on this device (KOReader's history), newest first, with
--- what their settings say: the home screen gives its own, else they are read once.
function Browser:localBooks()
    if self.locals then return self.locals end
    local books = {}
    local ok, ReadHistory = pcall(require, "readhistory")
    if ok and type(ReadHistory) == "table" then
        for _i, item in ipairs(ReadHistory.hist or {}) do
            if #books >= MAX_LOCAL then break end
            if item.file and lfs.attributes(item.file, "mode") == "file" then
                local book = localBook(item.file)
                -- Opening a book is not reading it: the history's time is only a guess
                book.time = book.read_at or item.time
                book.time_guess = book.read_at == nil
                books[#books + 1] = book
            end
        end
    end
    self.locals = books
    return books
end

--[[--
Every book known here or on the server, merged as the home screen merges
them (see mergeBooks), and found by the library's path of its entry, by
where it was downloaded and by title.
--]]
function Browser:bookIndex()
    local locals, remote = self:localBooks(), self:remoteIndex().list
    local index = self.book_index
    if index and index.locals == locals and index.remote == remote then return index end
    index = { locals = locals, remote = remote, by_lib = {}, by_path = {}, here = {} }
    local function localPathFor(entry)
        local path, downloaded = self:localFile(entry)
        return downloaded and path or nil
    end
    -- The server's books downloaded here but not among the last opened: what their settings say too
    local all, seen = {}, {}
    for _i, book in ipairs(locals) do
        all[#all + 1] = book
        seen[book.path] = true
    end
    for _i, r in ipairs(remote) do
        local path = type(r) == "table" and r.entry and localPathFor(r.entry)
        if path and not seen[path] then
            seen[path] = true
            local book = localBook(path)
            if book.percent ~= nil or book.finished then
                book.time = book.read_at
                all[#all + 1] = book
            end
        end
    end
    index.list = mergeBooks(all, remote, localPathFor)
    for _i, record in ipairs(index.list) do
        local lib = record.lib_path or libraryPath(record.entry)
        if lib then index.by_lib[lib] = record end
        if record.path then index.by_path[record.path] = record end
        if record.on_device then index.here[#index.here + 1] = record end
    end
    self.book_index = index
    return index
end

--- What is known of a book of the library (a record of mergeBooks), or nil.
function Browser:bookFor(entry)
    local index = self:bookIndex()
    local lib = libraryPath(entry)
    local record = lib and index.by_lib[lib]
    if record then return record end
    local path, downloaded = self:localFile(entry)
    if downloaded and index.by_path[path] then return index.by_path[path] end
    -- A book opened here from somewhere else, that the server does not know
    -- by its document: the same title and author
    for _i, here in ipairs(index.here) do
        if not here.lib_path and sameBook(entry, here) then return here end
    end
end

--- Where a book of the library is on this device (downloaded here, or the
--- same book opened from elsewhere), or nil.
function Browser:onDevice(entry)
    local path, downloaded = self:localFile(entry)
    if downloaded then return path end
    local record = self:bookFor(entry)
    if record and record.on_device and record.path and lfs.attributes(record.path, "mode") == "file" then
        return record.path
    end
end

--- Forgets what was read of the books here, after a download or a book read.
function Browser:forgetLocal()
    self.locals, self.book_index = nil, nil
end

--- How far a book of the library was read, here or on any device: { percent, finished }, or nil.
function Browser:progressFor(entry)
    local record = self:bookFor(entry)
    if record and (record.finished or (record.percent or 0) > 0) then
        return { percent = record.percent or 0, finished = record.finished or false }
    end
    -- Downloaded, but not among the last books opened here
    local path, downloaded = self:localFile(entry)
    local index = self:remoteIndex()
    local here = downloaded and localBook(path) or nil
    local lib = libraryPath(entry)
    local remote = (lib and index.by_path[lib]) or (here and here.document and index.by_document[here.document])
    if here and here.percent == nil and not here.finished then
        -- Downloaded but never opened here
        if not remote then return nil end
        here = nil
    end
    return mergeProgress(here, remote)
end

-- What the library says of a book: finished, how far read (here or on
-- another device), or only that it is on this device. ✓ is only for finished.
function Browser:readingNote(entry)
    local downloaded = self:onDevice(entry) ~= nil
    local state = self:progressFor(entry)
    if state and state.finished then
        return _("✓ Finished")
    end
    local parts = {}
    if state and state.percent > 0 then parts[1] = T(_("%1 % read"), math.floor(state.percent * 100 + 0.5)) end
    -- On this device, wherever it was opened from: in words, ✓ is only for finished books
    if downloaded then parts[#parts + 1] = _("Downloaded") end
    return #parts > 0 and table.concat(parts, " · ") or nil
end

-- How far the bar under a book goes, nil without one
function Browser:readBar(entry)
    local state = self:progressFor(entry)
    if state and not state.finished and state.percent > 0 then return state.percent end
end

function Browser:statusLine(entry)
    local parts = {}
    local acquisition = OPDS.bestAcquisition(entry)
    if acquisition then
        parts[#parts + 1] = formatName(acquisition)
        parts[#parts + 1] = formatSize(acquisition.length)
    end
    parts[#parts + 1] = self:readingNote(entry)
    local text = {}
    for _i, p in ipairs(parts) do
        if p and p ~= "" then text[#text + 1] = p end
    end
    return table.concat(text, " · ")
end

function Browser:listItem(entry, w, h)
    local sp = spacing()
    local padding = sp.M
    local inner_h = h - 2 * sp.G - Size.line.thin
    local text_x = padding
    local row = HorizontalGroup:new{ align = "center" }
    if entry.is_book then
        local cover_h = inner_h
        local cover_w = math.floor(cover_h / COVER_RATIO)
        table.insert(row, coverWidget(entry, self:cachedCover(entry), cover_w, cover_h, true))
        table.insert(row, HorizontalSpan:new{ width = sp.gap })
        text_x = text_x + cover_w + sp.gap
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
        local percent = self:readBar(entry)
        if percent then
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
            hold_callback = entry.is_book and function() self:bookActions(entry) end or nil,
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
    -- The same block in every cell, from its top: G, the cover's room, S, two
    -- lines of title and a line of status. A row's covers and titles line up
    -- whatever their sizes, and what is left stays under them.
    local sp = spacing()
    local caption_face = Font:getFace("cfont", 14)
    local status_face = Font:getFace("smallinfofont", 13)
    local probe = TextBoxWidget:new{ text = "Ag\nAg", face = caption_face, width = w, for_measurement_only = true }
    local caption_h = probe:getSize().h
    probe:free()
    local status_h = TextWidget:new{ text = "Ag", face = status_face }:getSize().h
    local status = entry.is_book and self:readingNote(entry)
    local room_h = math.max(1, h - 2 * sp.G - 2 * sp.S - caption_h - status_h)
    local cover_w = math.min(w, math.floor(room_h / COVER_RATIO))
    local cover_h = math.floor(cover_w * COVER_RATIO)
    local column = VerticalGroup:new{
        align = "center",
        VerticalSpan:new{ width = sp.G + room_h - cover_h },
        coverWidget(entry, entry.is_book and self:cachedCover(entry) or nil, cover_w, cover_h),
        VerticalSpan:new{ width = sp.S },
        TextBoxWidget:new{
            text = entry.title,
            face = caption_face,
            width = w,
            height = caption_h,
            height_overflow_show_ellipsis = true,
            alignment = "center",
        },
        VerticalSpan:new{ width = sp.S },
        status and TextWidget:new{ text = status, face = status_face, fgcolor = Blitbuffer.COLOR_DARK_GRAY, max_width = w }
            or VerticalSpan:new{ width = status_h },
        VerticalSpan:new{ width = sp.G },
    }
    return Item:new{
        width = w,
        height = h,
        callback = function() self:select(entry) end,
        hold_callback = entry.is_book and function() self:bookActions(entry) end or nil,
        LeftContainer:new{ dimen = Geom:new{ w = w, h = h }, VerticalGroup:new{ align = "center", column } },
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
    -- What is on this device may have changed since the last page
    self:forgetLocal()
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
            local row = HorizontalGroup:new{ align = "top", HorizontalSpan:new{ width = spacing().M } }
            for c = 1, layout.cols do
                local entry = level.entries[index]
                if entry then
                    if c > 1 then table.insert(row, HorizontalSpan:new{ width = spacing().gap }) end
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
            if not imageData(data) then
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

--[[--
A full-screen window takes the screen's new size after it turned or changed.
Android sends ScreenResize to every window, with SetDimensions just before
it; the size is read from the screen, so both events, or the same one twice,
lay the window out once. Returns whether the size changed.
--]]
function Browser.fitScreen(widget, dimen)
    local w, h = Screen:getWidth(), Screen:getHeight()
    if not (w and h) and type(dimen) == "table" then w, h = dimen.w, dimen.h end
    if not (w and h) then return false end
    local old = widget.dimen
    if old and old.w == w and old.h == h then return false end
    widget.dimen = Geom:new{ x = 0, y = 0, w = w, h = h }
    if widget.ges_events and widget.ges_events.Swipe then
        widget.ges_events.Swipe = { GestureRange:new{ ges = "swipe", range = widget.dimen } }
    end
    return true
end

--[[--
Turning the screen asked for while one of these windows is on top (a
gesture, a key, the gyro on Kobo and others): KOReader's file browser,
underneath, is what turns it, as when it is on top. Then every window hears
of the new size, as on Android. Returns false (not handled) without a file
browser that can do it, so KOReader goes on as it would.
--]]
function Browser.turnScreen(rotation)
    local ok, FileManager = pcall(require, "apps/filemanager/filemanager")
    local fm = ok and type(FileManager) == "table" and FileManager.instance or nil
    if not fm or type(fm.onSetRotationMode) ~= "function" then return false end
    local done, err = pcall(fm.onSetRotationMode, fm, rotation)
    if not done then
        logger.warn("OPDShelf: cannot turn the screen", err)
        return false
    end
    local size = Geom:new{ x = 0, y = 0, w = Screen:getWidth(), h = Screen:getHeight() }
    local Event = require("ui/event")
    UIManager:broadcastEvent(Event:new("ScreenResize", size))
    return true
end

function Browser:onScreenResize(dimen)
    if Browser.fitScreen(self, dimen) and not self.closed and self:currentLevel() then
        self:refresh()
    end
end
Browser.onSetDimensions = Browser.onScreenResize

function Browser:onSetRotationMode(rotation)
    return Browser.turnScreen(rotation)
end

-- Whatever turned the screen without saying so, the next paint has the right size
function Browser:paintTo(bb, x, y)
    if Browser.fitScreen(self) and self:currentLevel() then self:refresh() end
    InputContainer.paintTo(self, bb, x, y)
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
    self:loadProgress()
    UIManager:show(self, "flashui")
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
    local path, _downloaded, acquisition = self:localFile(entry)
    -- Here already, in the download folder or elsewhere
    local here = self:onDevice(entry)
    if not acquisition then
        UIManager:show(InfoMessage:new{ text = _("This book has no download.") })
        return
    end
    local function go()
        NetworkMgr:runWhenOnline(function()
            -- Never download inside the tap that asked for it. On e-ink (Kindle,
            -- Kobo) the screen is only refreshed once the tap returns to UIManager:
            -- a download that blocks there leaves a half drawn screen and no
            -- message until it ends. So it starts on the next tick, and runs in a
            -- subprocess so the screen keeps working and a tap can cancel it.
            UIManager:nextTick(function()
                Trapper:wrap(function() self:fetchBook(entry, acquisition, path, after) end)
            end)
        end)
    end
    if here then
        UIManager:show(ConfirmBox:new{
            text = T(_("%1 is already on this device."), entry.title),
            ok_text = _("Download again"),
            ok_callback = go,
            other_buttons = { { {
                text = _("Read"),
                callback = function() self:openBook(here) end,
            } } },
        })
        return
    end
    go()
end

-- The download itself, run from Browser:download inside Trapper:wrap.
function Browser:fetchBook(entry, acquisition, path, after)
    local size = formatSize(acquisition.length)
    local text = size and T(_("Downloading %1 (%2)…"), entry.title, size) or T(_("Downloading %1…"), entry.title)
    local message = InfoMessage:new{ text = text .. "\n\n" .. _("Tap to cancel.") }
    UIManager:show(message)
    UIManager:forceRePaint()
    util.makePath(self.plugin:downloadDir())
    local client = self.plugin.client
    local completed, ok, err = Trapper:dismissableRunInSubprocess(function()
        return client:download(acquisition.href, path)
    end, message)
    if not completed then
        -- Cancelled: the subprocess was killed half way
        os.remove(path .. ".part")
        UIManager:show(InfoMessage:new{ text = _("Download cancelled."), timeout = 2 })
        return
    end
    UIManager:close(message)
    if not ok or lfs.attributes(path, "mode") ~= "file" then
        UIManager:show(InfoMessage:new{ text = T(_("Could not download the book: %1"), tostring(err or "?")) })
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
    -- Out of the Trapper coroutine: opening a book from inside it is asking for trouble
    self:forgetLocal()
    UIManager:nextTick(function()
        if not self.closed and self.stack[1] then self:refresh() end
        if self.on_download then self.on_download(path) end
        if after then after(path) end
    end)
end

-- Long press on a book: what can be done with it, read or download it or
-- see its details, without opening them first
function Browser:bookActions(entry)
    local path = self:onDevice(entry)
    local downloaded = path ~= nil
    local dialog
    local function item(text, callback)
        return { { text = text, align = "left", callback = function()
            UIManager:close(dialog)
            callback()
        end } }
    end
    local buttons = {}
    if downloaded then
        buttons[#buttons + 1] = item(_("Read"), function() self:openBook(path) end)
    else
        buttons[#buttons + 1] = item(_("Download and read"), function()
            self:download(entry, function(p) self:openBook(p) end)
        end)
        buttons[#buttons + 1] = item(_("Download only"), function() self:quickDownload(entry) end)
    end
    buttons[#buttons + 1] = item(_("Book details"), function() self:showDetails(entry) end)
    dialog = ButtonDialog:new(Browser.holdDialog(entry.title, buttons))
    UIManager:show(dialog)
    return dialog
end

-- Download without opening the details, then offer to read it
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
    -- The screen's size: the home screen's browser is never shown, so its own may be old
    local w, h = Screen:getWidth(), Screen:getHeight()
    details.dimen = Geom:new{ x = 0, y = 0, w = w, h = h }
    local padding = spacing().M
    local close = function()
        UIManager:close(details, "flashui")
    end
    details.onClose = function() close() return true end
    -- Turned: laid out again for the new size
    local function relayout()
        if Screen:getWidth() == w and Screen:getHeight() == h then return end
        UIManager:close(details)
        self:showDetails(entry)
    end
    details.onScreenResize = relayout
    details.onSetDimensions = relayout
    details.onSetRotationMode = function(_self, rotation) return Browser.turnScreen(rotation) end
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
    local path = self:onDevice(entry)
    local downloaded = path ~= nil
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
    -- The cover as wide as a good part of the screen, and never taller than the
    -- room between the title and the buttons (a turned screen has little height)
    local cover_room = h - title_bar:getHeight() - button_table:getSize().h - 3 * padding
    local cover_h = math.max(1, math.min(math.floor(math.floor(w * 0.38) * COVER_RATIO), cover_room))
    local cover_w = math.floor(cover_h / COVER_RATIO)
    local cover_path = self:cachedCover(entry)
    -- The full size cover looks better here when it can be had quickly
    if entry.image and entry.image ~= entry.thumbnail and NetworkMgr:isOnline() then
        local big = self:coverPath(entry.image)
        if lfs.attributes(big, "mode") ~= "file" then
            local code, data = self.plugin.client:raw("GET", entry.image, nil, nil, nil, { 3, 8 })
            if code == 200 and imageData(data) then
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
    line(_("Language:"), languageName(entry.language))
    line(_("Publisher:"), entry.publisher)
    line(_("Published:"), publishedDate(entry.issued))
    if #entry.categories > 0 then line(_("Subjects:"), table.concat(entry.categories, ", ")) end
    local status = self:statusLine(entry)
    if status ~= "" then
        table.insert(info, VerticalSpan:new{ width = Size.padding.default })
        table.insert(info, TextBoxWidget:new{
            text = status, face = Font:getFace("cfont", 17), bold = downloaded, width = info_w,
        })
        local percent = self:readBar(entry)
        if percent then
            table.insert(info, VerticalSpan:new{ width = Size.padding.default })
            table.insert(info, ProgressWidget:new{
                width = info_w, height = Screen:scaleBySize(6), percentage = percent,
                margin_h = 0, margin_v = 0, radius = 0, bordersize = 0,
                bgcolor = Blitbuffer.COLOR_LIGHT_GRAY, fillcolor = Blitbuffer.COLOR_BLACK,
            })
        end
    end

    -- With the cover as tall as the room (a turned screen), the description goes
    -- beside it, under the details, rather than under it with no room left
    local beside_h = cover_room - info:getSize().h - padding - spacing().G
    local summary = entry.summary
    if summary and w > h and beside_h > Screen:scaleBySize(80) then
        table.insert(info, VerticalSpan:new{ width = spacing().G })
        table.insert(info, ScrollTextWidget:new{
            text = util.htmlToPlainTextIfHtml(entry.summary),
            face = Font:getFace("cfont", 17),
            width = info_w,
            height = beside_h,
            dialog = details,
        })
        -- The group was measured above: it has to place its children again
        if info.resetLayout then info:resetLayout() end
        summary = nil
    end
    local top_h = math.max(cover_h, info:getSize().h)
    local description_h = h - title_bar:getHeight() - top_h - button_table:getSize().h - 4 * padding
    local body = VerticalGroup:new{
        align = "left",
        title_bar,
        VerticalSpan:new{ width = padding },
        HorizontalGroup:new{
            align = "top",
            HorizontalSpan:new{ width = padding },
            coverWidget(entry, cover_path, cover_w, cover_h, "shrink"),
            HorizontalSpan:new{ width = padding },
            info,
        },
        VerticalSpan:new{ width = padding },
    }
    if summary and description_h > Screen:scaleBySize(40) then
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
    -- A refresh mode is needed: without one, e-ink screens (Kindle, Kobo) never
    -- show the new screen, only the bits other widgets happen to refresh.
    UIManager:show(details, "flashui")
end

--- What a book's hold menu is made of: wide enough for its title, which
--- takes two lines at most (otherwise a narrow dialog puts a word per line).
function Browser.holdDialog(title, buttons)
    local width = math.floor(math.min(Screen:getWidth(), Screen:getHeight()) * 0.8)
    return {
        -- ButtonDialog's title face with its default info style
        title = shortTitle(title, Font:getFace("infofont"), width - 4 * Size.padding.large, 2),
        title_align = "center",
        width = width,
        buttons = buttons,
    }
end

-- Shared with the home screen
Browser.languageName = languageName
Browser.formatSize = formatSize
Browser.readState = readState
Browser.shortTitle = shortTitle
Browser.coverWidget = coverWidget
Browser.Item = Item
Browser.readPercent = readPercent
Browser.localBook = localBook
Browser.libraryPath = libraryPath
Browser.mergeProgress = mergeProgress
Browser.mergeBooks = mergeBooks
Browser.sameBook = sameBook
Browser.plainText = plainText
Browser.publishedDate = publishedDate
Browser.imageData = imageData
Browser.isImage = isImage
Browser.blankImage = blankImage
Browser.MAX_LOCAL = MAX_LOCAL
Browser.seriesLine = seriesLine
Browser.COVER_RATIO = COVER_RATIO

return Browser

--[[--
OPDShelf for KOReader.

- Syncs reading positions with an OPDShelf server in real time, with its own
  protocol (no need for KOReader's Progress sync): the position is sent a few
  seconds after each page turn, kept while offline, and changes made on other
  devices arrive at once over a live connection while the book is open.
- Resolves conflicts: a device that is further ahead can move the book there
  by itself, one that is behind asks, and when two devices both read since
  they last agreed (for example one of them offline) it lets you choose.
- "Set as last position" in the text selection menu sends the exact
  sentence, so every device opens the book right there.
- A library browser for the server's OPDS catalog, with covers as a list or
  a grid, search, book details and downloads.
- A home screen that can replace KOReader's start screen: the book to
  continue, the newest books on the server and the last ones read here.

@module koplugin.opdshelf
--]]

local ButtonDialog = require("ui/widget/buttondialog")
local ConfirmBox = require("ui/widget/confirmbox")
local DataStorage = require("datastorage")
local Device = require("device")
local Dispatcher = require("dispatcher")
local Event = require("ui/event")
local InfoMessage = require("ui/widget/infomessage")
local InputDialog = require("ui/widget/inputdialog")
local JSON = require("json")
local LuaSettings = require("luasettings")
local MultiConfirmBox = require("ui/widget/multiconfirmbox")
local MultiInputDialog = require("ui/widget/multiinputdialog")
local NetworkMgr = require("ui/network/manager")
local Notification = require("ui/widget/notification")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local logger = require("logger")
local util = require("util")
local Client = require("opdshelf_client")
local Stream = require("opdshelf_stream")
local Sync = require("opdshelf_sync")
local T = require("ffi/util").template
local _ = require("opdshelf_i18n")

-- Words of text saved before and after a position, to find it again if the
-- xpointer does not lead to the same words in another reader
local CONTEXT_WORDS = 8
local HIGHLIGHT_SECONDS = 6
-- Seconds after the last page turn before the position is sent
local PUSH_DELAY = 3
-- How often the live connection is read while a book is open
local STREAM_POLL = 2
-- Waits before reconnecting the live connection, in seconds
local RETRY = { 5, 15, 30, 60, 120, 300 }

local OPDShelf = WidgetContainer:extend{
    name = "opdshelf",
    is_doc_only = false,
}

-- Cuts a UTF-8 string to at most `max` bytes without splitting a character.
local function utf8Head(s, max)
    if #s <= max then return s end
    local cut = max
    while cut > 0 do
        local byte = s:byte(cut + 1)
        if not byte or byte < 0x80 or byte >= 0xC0 then break end
        cut = cut - 1
    end
    return s:sub(1, cut)
end

-- Set when the home screen is asked for from a book, to show it in the file browser that follows
local home_requested = false

local function urlEncode(s)
    return (tostring(s):gsub("[^%w%-%._~]", function(c) return string.format("%%%02X", c:byte()) end))
end

local function percent(value)
    return T(_("%1 %"), math.floor((value or 0) * 100 + 0.5))
end

function OPDShelf:init()
    OPDShelf.instance = self
    self.state = LuaSettings:open(DataStorage:getSettingsDir() .. "/opdshelf.lua")
    self.device_id = G_reader_settings:readSetting("device_id")
    self:migrateAccount()
    self:makeClient()
    self:onDispatcherRegisterActions()
    if self.ui.menu then
        self.ui.menu:registerToMainMenu(self)
    end
    if not self.ui.document then
        self:patchFileManager()
        -- The file browser: the home screen opens over it at start and after closing
        -- a book, like a launcher
        UIManager:nextTick(function()
            local wanted = home_requested or self:launcherEnabled()
            home_requested = false
            if self.ui.file_chooser and wanted then
                self:showHome()
            end
        end)
    end
    if self.ui.highlight and self.ui.document then
        self.ui.highlight:addToHighlightDialog("12_opdshelf_last_position", function(highlight)
            return {
                text = _("Set as last position"),
                show_in_highlight_dialog_func = function()
                    return not highlight.ui.document.info.has_pages
                end,
                callback = function()
                    local selected = highlight.selected_text
                    highlight:onClose()
                    self:setLastPosition(selected)
                end,
            }
        end)
    end
end

function OPDShelf:onDispatcherRegisterActions()
    Dispatcher:registerAction("opdshelf_home", {
        category = "none", event = "OPDShelfHome", title = _("OPDShelf home"), general = true,
    })
    Dispatcher:registerAction("opdshelf_library", {
        category = "none", event = "OPDShelfLibrary", title = _("OPDShelf library"), general = true,
    })
    Dispatcher:registerAction("opdshelf_sync", {
        category = "none", event = "OPDShelfSync", title = _("OPDShelf: sync this book now"), reader = true,
    })
end

-- ---------------------------------------------------------------------------
-- Account

-- The first time, take the server and user from Progress sync if it pointed to OPDShelf
function OPDShelf:migrateAccount()
    if self.state:readSetting("server") then return end
    local kosync = LuaSettings:open(DataStorage:getSettingsDir() .. "/kosync.lua"):readSetting("settings") or {}
    local server = kosync.custom_server
    if type(server) == "string" and server:match("/kosync/*$") then
        self.state:saveSetting("server", (server:gsub("/kosync/*$", "")))
        self.state:saveSetting("username", kosync.username)
        self.state:flush()
    end
end

function OPDShelf:makeClient()
    self.client = Client.new{
        server = self.state:readSetting("server") or "",
        username = self.state:readSetting("username"),
        password = self.state:readSetting("password"),
    }
end

function OPDShelf:deviceName()
    return self.state:readSetting("device_name") or Device.model
end

function OPDShelf:ready()
    return self.client:configured() and (self.state:readSetting("password") or "") ~= ""
end

function OPDShelf:editAccount(done)
    local dialog
    dialog = MultiInputDialog:new{
        title = _("OPDShelf server"),
        fields = {
            {
                description = _("Server address"),
                text = self.state:readSetting("server") or "",
                hint = "https://books.example.com",
            },
            {
                description = _("Username"),
                text = self.state:readSetting("username") or "",
                hint = _("Username"),
            },
            {
                description = _("Password"),
                text = self.state:readSetting("password") or "",
                text_type = "password",
                hint = _("Password"),
            },
        },
        buttons = { {
            { text = _("Cancel"), id = "close", callback = function() UIManager:close(dialog) end },
            { text = _("Save"), callback = function()
                local fields = dialog:getFields()
                UIManager:close(dialog)
                local server = util.trim(fields[1] or ""):gsub("/+$", ""):gsub("/kosync$", "")
                if server ~= "" and not server:match("^https?://") then
                    server = "http://" .. server
                end
                self.state:saveSetting("server", server)
                self.state:saveSetting("username", util.trim(fields[2] or ""))
                self.state:saveSetting("password", fields[3] or "")
                self.state:flush()
                self:makeClient()
                NetworkMgr:runWhenOnline(function() self:checkAccount(done) end)
            end },
        } },
    }
    UIManager:show(dialog)
    dialog:onShowKeyboard()
end

function OPDShelf:checkAccount(done)
    local code, body = self.client:json_request("GET", "/api/progress/0")
    local text
    if code == 200 or code == 400 then
        text = _("Connected to OPDShelf.")
        self:stopStream()
        if self:hasBook() then
            self:pull("open")
            self:startStream()
        end
        if done then done() end
    elseif code == 401 then
        text = _("The server did not accept the username or password.")
    elseif code then
        text = T(_("This does not look like an OPDShelf server (it answered %1)."), code)
    else
        text = T(_("Cannot reach the server: %1"), tostring(body))
    end
    UIManager:show(InfoMessage:new{ text = text, timeout = 4 })
end

-- Runs `action` once the account is set up, asking for it first if needed
function OPDShelf:withAccount(action)
    if self:ready() then
        action()
    else
        self:editAccount(action)
    end
end

-- ---------------------------------------------------------------------------
-- Positions

function OPDShelf:hasBook()
    return self.ui.document ~= nil and self.document ~= nil
end

function OPDShelf:docState()
    if not self.doc_state then
        self.doc_state = self.ui.doc_settings:readSetting("opdshelf_sync") or {}
    end
    return self.doc_state
end

function OPDShelf:saveDocState()
    self.ui.doc_settings:saveSetting("opdshelf_sync", self.doc_state)
end

function OPDShelf:digest()
    local id = self.ui.doc_settings:readSetting("partial_md5_checksum")
    if not id and self.ui.document.file then
        id = util.partialMD5(self.ui.document.file)
    end
    return id
end

function OPDShelf:serverNow()
    return os.time() + (self.server_offset or 0)
end

function OPDShelf:noteServerTime(body)
    if type(body) == "table" and type(body.server_time) == "number" then
        self.server_offset = body.server_time - os.time()
    end
end

-- Text around a position
function OPDShelf:contextAt(xp)
    local doc = self.ui.document
    local first, last = xp, xp
    for i = 1, CONTEXT_WORDS do -- luacheck: no unused
        local previous = doc:getPrevVisibleWordStart(first)
        if not previous then break end
        first = previous
    end
    for i = 1, CONTEXT_WORDS do -- luacheck: no unused
        local following = doc:getNextVisibleWordEnd(last)
        if not following then break end
        last = following
    end
    return {
        before = first ~= xp and doc:getTextFromXPointers(first, xp) or "",
        after = last ~= xp and doc:getTextFromXPointers(xp, last) or "",
    }
end

function OPDShelf:percentageAt(xp)
    local doc = self.ui.document
    local ok, pos = pcall(doc.getPosFromXPointer, doc, xp)
    local height = doc.info and doc.info.doc_height
    if ok and pos and height and height > 0 then
        return math.max(0, math.min(1, pos / height))
    end
    return self.ui.rolling and self.ui.rolling:getLastPercent() or 0
end

--- Where the reader is now: progress (xpointer or page) and percentage.
function OPDShelf:position()
    local doc = self.ui.document
    if doc.info.has_pages then
        return tostring(self.ui.paging:getLastProgress()), self.ui.paging:getLastPercent() or 0
    end
    -- While a point set by hand is on screen, it is the position, not the top of the page
    if self.mark and doc:isXPointerInCurrentPage(self.mark) then
        return self.mark, self:percentageAt(self.mark)
    end
    return self.ui.rolling:getLastProgress(), self.ui.rolling:getLastPercent() or 0
end

function OPDShelf:validXPointer(xp)
    if type(xp) ~= "string" or xp:sub(1, 1) ~= "/" then return false end
    local doc = self.ui.document
    if not doc.isXPointerInDocument then return true end
    local ok, valid = pcall(doc.isXPointerInDocument, doc, xp)
    return ok and valid
end

--- Whether a position from the server is on the page shown now.
function OPDShelf:showing(record)
    if type(record) ~= "table" or type(record.progress) ~= "string" then return false end
    local doc = self.ui.document
    if doc.info.has_pages then
        return tonumber(record.progress) == self.ui.paging:getLastProgress()
    end
    if self:validXPointer(record.progress) then
        return doc:isXPointerInCurrentPage(record.progress)
    end
    local here = select(2, self:position())
    return math.abs((record.percentage or 0) - here) < 0.002
end

--- Goes to a position from the server.
function OPDShelf:goTo(record)
    self.following = record
    self.following_at = nil
    local doc = self.ui.document
    if doc.info.has_pages then
        local page = tonumber(record.progress)
        if page then
            self.ui:handleEvent(Event:new("GotoPage", page))
        else
            self.ui:handleEvent(Event:new("GotoPercent", (record.percentage or 0) * 100))
        end
    elseif self:validXPointer(record.progress) then
        if record.manual then
            self:setMark(record.progress)
            local selected = type(record.context) == "table" and record.context.selected or ""
            self.pending_highlight = { xp = record.progress, selected = selected }
        end
        self.ui:handleEvent(Event:new("GotoXPointer", record.progress, record.progress))
    else
        -- The page reached may not contain the exact percentage: its first page change is ours
        self.following_at = os.time()
        self.ui:handleEvent(Event:new("GotoPercent", (record.percentage or 0) * 100))
    end
    self:agree(record)
end

-- This device and the server now agree on `record`
function OPDShelf:agree(record)
    local state = self:docState()
    state.synced = { timestamp = record.timestamp, device_id = record.device_id, progress = record.progress }
    state.moved = false
    self.last_sent = self:position()
    self:saveDocState()
end

-- The server's position was seen and left aside; turning a page makes ours win
function OPDShelf:dismiss(record)
    local state = self:docState()
    state.synced = { timestamp = record.timestamp, device_id = record.device_id, progress = record.progress }
    self:saveDocState()
end

function OPDShelf:describe(record)
    local device = record.device ~= "" and record.device or _("another device")
    return T(_("%1: %2, %3"), device, percent(record.percentage), Sync.ago(record.timestamp, self:serverNow(), _, T))
end

function OPDShelf:closePrompt()
    if self.prompt then
        UIManager:close(self.prompt)
        self.prompt = nil
    end
end

--- Acts on the server's position of the open book.
function OPDShelf:handleRemote(remote, reason)
    if not self:hasBook() or self.closed then return end
    local state = self:docState()
    local here = select(2, self:position())
    local action = Sync.decide{
        remote = remote,
        me = self.device_id,
        synced = state.synced,
        moved = state.moved or (self.state:readSetting("pending") or {})[self.document] ~= nil,
        showing = self:showing(remote),
        ahead = type(remote) == "table" and (remote.percentage or 0) > here,
        reason = reason,
        idle = self.last_move and (os.time() - self.last_move) or nil,
        forward = self.state:readSetting("forward") or "jump",
        backward = self.state:readSetting("backward") or "ask",
    }
    logger.dbg("OPDShelf: server position", reason, action)
    if action == "none" then
        if type(remote) == "table" and remote.progress then
            if remote.manual and remote.device_id ~= self.device_id and self:showing(remote)
                    and not (state.synced and state.synced.timestamp == remote.timestamp) then
                -- A point set on another device on this very page: show where it is
                self:goTo(remote)
                UIManager:nextTick(function() self:showMark() end)
            elseif remote.device_id == self.device_id or self:showing(remote) then
                -- Same place: agreed. Our own position coming back: only seen
                if self:showing(remote) then self:agree(remote) else self:dismiss(remote) end
            elseif reason ~= "manual" then
                self:dismiss(remote)
            end
        end
        if reason == "manual" then
            UIManager:show(InfoMessage:new{ text = _("This book is up to date."), timeout = 2 })
        end
    elseif action == "push" then
        -- Not while the reader is choosing between this position and another one
        if not self.prompt then self:push{ force = true } end
    elseif action == "jump" then
        self:goTo(remote)
        Notification:notify(T(_("Continuing from %1"), self:describe(remote)), Notification.SOURCE_ALWAYS_SHOW)
    elseif action == "ask" then
        self:ask(remote)
    elseif action == "conflict" then
        self:askConflict(remote)
    end
end

function OPDShelf:ask(remote)
    self:closePrompt()
    -- Asked once: the same position does not ask again
    self:dismiss(remote)
    local text
    if remote.manual then
        text = T(_("A point to continue this book from was set on %1. Go there?"), self:describe(remote))
    else
        local here = select(2, self:position())
        text = (remote.percentage or 0) >= here
            and T(_("Further ahead on %1. Go there?"), self:describe(remote))
            or T(_("Behind on %1. Go back there?"), self:describe(remote))
    end
    self.prompt = ConfirmBox:new{
        text = text,
        ok_text = _("Go"),
        cancel_text = _("Stay here"),
        ok_callback = function()
            self.prompt = nil
            self:goTo(remote)
        end,
        cancel_callback = function()
            self.prompt = nil
            -- Other devices will continue from here
            self:push{ force = true, now = true }
        end,
    }
    UIManager:show(self.prompt)
end

function OPDShelf:askConflict(remote)
    self:closePrompt()
    self:dismiss(remote)
    local here = select(2, self:position())
    local state = self:docState()
    local read_here = self.last_move or state.read_at
    self.prompt = MultiConfirmBox:new{
        text = T(_("This book was read here and on another device since they last synced.\n\nHere: %1%2\n%3\n\nWhere do you want to continue?"),
            percent(here),
            read_here and (", " .. Sync.ago(read_here + (self.server_offset or 0), self:serverNow(), _, T)) or "",
            self:describe(remote)),
        choice1_text = _("Here"),
        choice1_callback = function()
            self.prompt = nil
            self:push{ force = true, now = true }
        end,
        choice2_text = remote.device ~= "" and remote.device or _("Other device"),
        choice2_callback = function()
            self.prompt = nil
            self:goTo(remote)
        end,
        cancel_callback = function()
            self.prompt = nil
        end,
    }
    UIManager:show(self.prompt)
end

--- Queues the current position and sends it.
-- opts.force: send it even if it did not change; opts.now: as read now (it wins);
-- opts.manual, opts.context: a point set by hand
function OPDShelf:push(opts)
    opts = opts or {}
    if not self:hasBook() or not self:ready() then return end
    local progress, percentage = self:position()
    if not progress then return end
    if not opts.force and progress == self.last_sent then return end
    local pending = self.state:readSetting("pending") or {}
    pending[self.document] = {
        document = self.document,
        progress = progress,
        percentage = percentage,
        device = self:deviceName(),
        device_id = self.device_id,
        manual = opts.manual or nil,
        context = opts.context,
        timestamp = (not opts.now and (self.last_move or self:docState().read_at)) or os.time(),
    }
    self.state:saveSetting("pending", pending)
    self.state:flush()
    self.last_sent = progress
    if NetworkMgr:isOnline() then
        return self:flush(opts.interactive)
    elseif opts.interactive then
        UIManager:show(InfoMessage:new{
            text = _("Saved. It will be sent when the device is online."),
            timeout = 3,
        })
    end
end

--- Sends the queued positions (each book keeps only its latest one).
function OPDShelf:flush(interactive)
    if self.flushing or not self:ready() then return false end
    local pending = self.state:readSetting("pending") or {}
    if next(pending) == nil then return false end
    self.flushing = true
    local sent, failed, rejected, answer = 0, false, false, nil
    for document, item in pairs(pending) do
        item.sent_at = os.time()
        local code, body = self.client:putProgress(document, item)
        if code == 200 then
            pending[document] = nil
            sent = sent + 1
            self:noteServerTime(body)
            if document == self.document then answer = body end
        elseif code == 401 then
            rejected = true
            break
        elseif code and code >= 400 and code < 500 then
            -- The server will never accept it
            logger.warn("OPDShelf: position rejected with", code)
            pending[document] = nil
        else
            failed = true
            break
        end
    end
    self.state:saveSetting("pending", pending)
    self.state:flush()
    self.flushing = false
    local handled = answer ~= nil
    if answer and type(answer.current) == "table" and self:hasBook() then
        if answer.accepted == false then
            -- Another device read further on in the meantime
            self:handleRemote(answer.current, "rejected")
        else
            local state = self:docState()
            state.synced = { timestamp = answer.current.timestamp, device_id = answer.current.device_id,
                progress = answer.current.progress }
            state.moved = false
            self:saveDocState()
        end
    end
    if rejected then
        UIManager:show(InfoMessage:new{
            text = _("The OPDShelf server did not accept the username or password. Check them in OPDShelf → Server and account."),
        })
    elseif interactive and failed then
        UIManager:show(InfoMessage:new{ text = _("Cannot reach the server. It will be sent later."), timeout = 3 })
    elseif interactive and sent > 0 then
        UIManager:show(InfoMessage:new{ text = _("Last position set. Other devices will open the book here."), timeout = 3 })
    end
    return handled
end

--- Asks the server for the book's position.
function OPDShelf:pull(reason)
    if not self:hasBook() or not self:ready() or not NetworkMgr:isOnline() then return end
    local code, body = self.client:getProgress(self.document)
    if code == 401 then
        if reason == "manual" then
            UIManager:show(InfoMessage:new{ text = _("The server did not accept the username or password.") })
        end
        return
    elseif code ~= 200 or type(body) ~= "table" then
        if reason == "manual" then
            UIManager:show(InfoMessage:new{ text = _("Cannot reach the server."), timeout = 3 })
        end
        return
    end
    self:noteServerTime(body)
    self.history = body.history
    self:handleRemote(body.current, reason)
end

--- Sends what is queued, then gets the server's position.
function OPDShelf:syncNow(reason)
    -- The answer to sending this book's position already says where it stands
    if not self:flush() then
        self:pull(reason)
    end
end

function OPDShelf:onOPDShelfSync()
    if not self:hasBook() then return end
    self:withAccount(function()
        NetworkMgr:runWhenOnline(function()
            if self:docState().moved then self:push{ force = true } end
            self:pull("manual")
        end)
    end)
    return true
end

-- A position set by hand from the text selection
function OPDShelf:setLastPosition(selected)
    local xp = selected and selected.pos0
    if type(xp) ~= "string" then return end
    self:withAccount(function()
        local context = self:contextAt(xp)
        context.selected = utf8Head(util.cleanupSelectedText(selected.text or ""), 200)
        context.before = utf8Head(context.before, 400)
        context.after = utf8Head(context.after, 400)
        -- Until this page is left, this point is the position instead of the top of the page
        self:setMark(xp)
        self:closePrompt()
        self:push{ force = true, now = true, manual = true, context = context, interactive = true }
    end)
end

function OPDShelf:setMark(xp)
    self.mark = xp
    if xp then
        self.ui.doc_settings:saveSetting("opdshelf_mark", xp)
    else
        self.ui.doc_settings:delSetting("opdshelf_mark")
    end
end

-- Highlights the marked passage for a few seconds
function OPDShelf:showMark()
    local mark = self.pending_highlight
    self.pending_highlight = nil
    if not mark or self.closed then return end
    local doc = self.ui.document
    local last = mark.xp
    local wanted = math.max(#(mark.selected or ""), 1)
    for i = 1, 40 do -- luacheck: no unused
        local following = doc:getNextVisibleWordEnd(last)
        if not following then break end
        last = following
        if #(doc:getTextFromXPointers(mark.xp, last) or "") >= wanted then break end
    end
    if last == mark.xp then return end
    doc:getTextFromXPointers(mark.xp, last, true)
    UIManager:setDirty(self.dialog, "ui")
    self.clear_task = function()
        if self.closed then return end
        doc:clearSelection()
        UIManager:setDirty(self.dialog, "ui")
    end
    UIManager:scheduleIn(HIGHLIGHT_SECONDS, self.clear_task)
end

-- While Progress sync also syncs to this server, its page-top positions would
-- undo exact points: it sends the mark instead while it is on screen.
function OPDShelf:patchProgressSync()
    local kosync = self.ui.kosync
    if not kosync or not kosync.getLastProgress then return end
    local original = kosync.opdshelf_original or kosync.getLastProgress
    kosync.opdshelf_original = original
    kosync.getLastProgress = function(this)
        local mark = self.mark
        if mark and not self.ui.document.info.has_pages and self.ui.document:isXPointerInCurrentPage(mark) then
            return mark
        end
        return original(this)
    end
end

-- Progress sync syncing on its own at the same time only gets in the way
function OPDShelf:offerToStopProgressSync()
    local kosync = self.ui.kosync
    if not (kosync and kosync.settings and kosync.settings.auto_sync) then return end
    if self.state:readSetting("kosync_offer_done") then return end
    self.state:saveSetting("kosync_offer_done", true)
    self.state:flush()
    UIManager:show(ConfirmBox:new{
        text = _("OPDShelf now syncs reading positions by itself, live. Turn off automatic Progress sync so the two do not compete?"),
        ok_text = _("Turn off"),
        ok_callback = function()
            pcall(kosync.onKOSyncToggleAutoSync, kosync, false)
        end,
    })
end

-- ---------------------------------------------------------------------------
-- Live updates

function OPDShelf:liveEnabled()
    return self.state:readSetting("live") ~= false
end

function OPDShelf:startStream()
    if self.stream or self.closed or not self:hasBook() or not self:ready() or not self:liveEnabled() then return end
    if not NetworkMgr:isOnline() then return end
    UIManager:unschedule(self.retry_task)
    local reconnecting = self.stream_failures and self.stream_failures > 0
    self.stream = Stream.new{
        -- Who is listening, so the server's Devices page shows this one as connected now
        url = self.client.server .. "/api/progress/events?documents=" .. self.document
            .. "&device_id=" .. urlEncode(self.device_id or "") .. "&device=" .. urlEncode(self:deviceName()),
        headers = { ["Authorization"] = self.client:authorization(), ["User-Agent"] = "OPDShelf-KOReader" },
        on_open = function()
            self.stream_failures = 0
            -- Catch up with what changed while disconnected
            if reconnecting then self:pull("live") end
        end,
        on_event = function(name, data)
            self:onStreamEvent(name, data)
        end,
    }
    self.stream_task = self.stream_task or function() self:stepStream() end
    UIManager:scheduleIn(0.5, self.stream_task)
end

function OPDShelf:stepStream()
    local stream = self.stream
    if not stream then return end
    local alive, err, status = stream:step()
    if alive then
        UIManager:scheduleIn(STREAM_POLL, self.stream_task)
        return
    end
    self.stream = nil
    logger.dbg("OPDShelf: live connection ended:", err)
    if status == 401 or status == 404 or self.closed then return end
    self.stream_failures = (self.stream_failures or 0) + 1
    local wait = RETRY[math.min(self.stream_failures, #RETRY)]
    self.retry_task = self.retry_task or function() self:startStream() end
    UIManager:scheduleIn(wait, self.retry_task)
end

function OPDShelf:stopStream()
    if self.stream_task then UIManager:unschedule(self.stream_task) end
    if self.retry_task then UIManager:unschedule(self.retry_task) end
    if self.stream then
        self.stream:close()
        self.stream = nil
    end
    self.stream_failures = 0
end

function OPDShelf:onStreamEvent(name, data)
    if name == "hello" then
        local ok, body = pcall(JSON.decode, data)
        if ok then self:noteServerTime(body) end
    elseif name == "progress" then
        local ok, change = pcall(JSON.decode, data)
        if ok and type(change) == "table" and change.document == self.document then
            self:handleRemote(change.current, "live")
        end
    elseif name == "resync" then
        self:pull("live")
    end
end

-- ---------------------------------------------------------------------------
-- Reader events

function OPDShelf:onReaderReady()
    if not self.ui.document then return end
    self.document = self:digest()
    if not self.document then return end
    self.mark = self.ui.doc_settings:readSetting("opdshelf_mark")
    self.last_sent = self:position()
    self:patchProgressSync()
    self.push_task = function() self:push() end
    self.started = true
    if not self:ready() then return end
    UIManager:scheduleIn(1, function()
        if self.closed then return end
        self:offerToStopProgressSync()
        if NetworkMgr:isOnline() then
            self:syncNow("open")
            self:startStream()
        end
    end)
end

function OPDShelf:onMoved()
    if not self.started or self.closed then return end
    local doc = self.ui.document
    if not doc.info.has_pages then
        if self.pending_highlight and doc:isXPointerInCurrentPage(self.pending_highlight.xp) then
            -- Wait for the page to be drawn
            UIManager:nextTick(function() self:showMark() end)
        elseif self.mark and not doc:isXPointerInCurrentPage(self.mark) then
            -- Reading went on: the page is the position again
            self:setMark(nil)
        end
    end
    -- Moving to a position from another device is not reading
    if self.following and (self:showing(self.following) or (self.following_at and os.time() - self.following_at < 2)) then
        self.following_at = nil
        return
    end
    self.following = nil
    self.last_move = os.time()
    local state = self:docState()
    if not state.moved then
        state.moved = true
        self:saveDocState()
    end
    state.read_at = self.last_move
    if self.push_task then
        UIManager:unschedule(self.push_task)
        UIManager:scheduleIn(PUSH_DELAY, self.push_task)
    end
end

function OPDShelf:onPageUpdate()
    self:onMoved()
end

function OPDShelf:onPosUpdate()
    self:onMoved()
end

-- Sends a position not sent yet right away (closing, sleeping, losing the network)
function OPDShelf:pushNow()
    if not self:hasBook() then return end
    if self.push_task then UIManager:unschedule(self.push_task) end
    if self:docState().moved then
        self:push()
    end
end

function OPDShelf:onNetworkConnected()
    UIManager:scheduleIn(1, function()
        if self.closed then return end
        self:flush(false)
        if self:hasBook() then
            self:pull("open")
            self:startStream()
        end
    end)
end

function OPDShelf:onNetworkDisconnecting()
    self:pushNow()
    self:stopStream()
end

function OPDShelf:onSuspend()
    self:pushNow()
    self:stopStream()
end

function OPDShelf:onResume()
    if not self:hasBook() then return end
    UIManager:scheduleIn(2, function()
        if self.closed or not NetworkMgr:isOnline() then return end
        self:syncNow("open")
        self:startStream()
    end)
end

function OPDShelf:onCloseDocument()
    self:pushNow()
    self.closed = true
    self:stopStream()
    self:closePrompt()
    if self.push_task then UIManager:unschedule(self.push_task) end
    if self.clear_task then UIManager:unschedule(self.clear_task) end
end

-- ---------------------------------------------------------------------------
-- Library

function OPDShelf:downloadDir()
    local dir = self.state:readSetting("download_dir")
        or G_reader_settings:readSetting("download_dir")
        or G_reader_settings:readSetting("home_dir")
        or Device.home_dir
        or DataStorage:getDataDir()
    return (dir:gsub("/+$", ""))
end

function OPDShelf:chooseDownloadDir(done)
    local PathChooser = require("ui/widget/pathchooser")
    UIManager:show(PathChooser:new{
        select_directory = true,
        select_file = false,
        path = self:downloadDir(),
        onConfirm = function(path)
            self.state:saveSetting("download_dir", path)
            self.state:flush()
            if done then done() end
        end,
    })
end

--- Opens the library: where it was left, at `url` if given, or searching.
function OPDShelf:showLibrary(search, url, title)
    self:withAccount(function()
        if self.browser then
            if search then self.browser:search() end
            return
        end
        local Browser = require("opdshelf_browser")
        self.browser = Browser:new{ plugin = self }
        local session = self.library_session
        if url then
            self.browser:start(url, title)
            if session then self.browser.root = session.root end
        elseif session then
            self.browser:resume(session)
        else
            self.browser:start()
        end
        if search then self.browser:search() end
    end)
end

function OPDShelf:onOPDShelfLibrary()
    self:showLibrary()
    return true
end

function OPDShelf:showHome()
    if self.home then return end
    if self.ui.document then
        -- From a book: go to the file browser, as KOReader's own start screens do,
        -- and the plugin there shows the home screen
        home_requested = true
        self.ui:onHome()
        return
    end
    local Home = require("opdshelf_home")
    self.home = Home:new{ plugin = self }
    self.home:show()
end

function OPDShelf:onOPDShelfHome()
    self:showHome()
    return true
end

--- Whether the home screen opens at start and after closing a book (on unless turned off).
function OPDShelf:launcherEnabled()
    local start_with = G_reader_settings:readSetting("start_with") or "filemanager"
    if start_with == "opdshelf" then return true end
    -- On by default, unless KOReader was set to start with something else
    local on = self.state:readSetting("launcher")
    if on == nil then return start_with == "filemanager" end
    return on
end

function OPDShelf:setLauncher(on)
    self.state:saveSetting("launcher", on)
    self.state:flush()
    local start_with = G_reader_settings:readSetting("start_with")
    if on then
        G_reader_settings:saveSetting("start_with", "opdshelf")
    elseif start_with == "opdshelf" then
        G_reader_settings:saveSetting("start_with", "filemanager")
    end
end

-- Once per KOReader run: the file browser's home button opens the home screen,
-- and "Start with" lists it even if its menu is built elsewhere
function OPDShelf:patchFileManager()
    local ok, FileManager = pcall(require, "apps/filemanager/filemanager")
    if ok and type(FileManager) == "table" and FileManager.onHome and not FileManager._opdshelf_onHome then
        local original = FileManager.onHome
        FileManager._opdshelf_onHome = original
        FileManager.onHome = function(fm, ...)
            local plugin = rawget(fm, "opdshelf")
            if plugin and plugin.launcherEnabled and plugin:launcherEnabled() then
                plugin:showHome()
                return true
            end
            return original(fm, ...)
        end
    end
    local okm, FileManagerMenu = pcall(require, "apps/filemanager/filemanagermenu")
    if okm and type(FileManagerMenu) == "table" and FileManagerMenu.getStartWithMenuTable and not FileManagerMenu._opdshelf_start_with then
        local original = FileManagerMenu.getStartWithMenuTable
        FileManagerMenu._opdshelf_start_with = original
        FileManagerMenu.getStartWithMenuTable = function(...)
            local menu = original(...)
            OPDShelf.addStartWith(nil, { start_with = menu })
            return menu
        end
    end
end

-- Adds "OPDShelf home" to File browser → Settings → Start with
function OPDShelf:addStartWith(menu_items)
    local start_with = menu_items.start_with
    if not start_with or not start_with.sub_item_table or start_with.opdshelf then return end
    start_with.opdshelf = true
    table.insert(start_with.sub_item_table, {
        text = _("OPDShelf home"),
        checked_func = function() return G_reader_settings:readSetting("start_with") == "opdshelf" end,
        callback = function()
            G_reader_settings:saveSetting("start_with", "opdshelf")
            local plugin = OPDShelf.instance
            if plugin then plugin.state:saveSetting("launcher", true) plugin.state:flush() end
        end,
        radio = true,
    })
    -- Choosing another start screen turns the launcher off
    for _i, item in ipairs(start_with.sub_item_table) do
        local callback = item.callback
        if callback and item.text ~= _("OPDShelf home") then
            item.callback = function(...)
                local plugin = OPDShelf.instance
                if plugin then plugin.state:saveSetting("launcher", false) plugin.state:flush() end
                return callback(...)
            end
        end
    end
    local text_func = start_with.text_func
    start_with.text_func = function()
        if G_reader_settings:readSetting("start_with") == "opdshelf" then
            return T(_("Start with: %1"), _("OPDShelf home"))
        end
        return text_func and text_func() or start_with.text
    end
end

-- ---------------------------------------------------------------------------
-- Menu

function OPDShelf:showPositions()
    NetworkMgr:runWhenOnline(function()
        local code, body = self.client:getProgress(self.document)
        if code ~= 200 or type(body) ~= "table" then
            UIManager:show(InfoMessage:new{ text = _("Cannot reach the server."), timeout = 3 })
            return
        end
        self:noteServerTime(body)
        -- The latest position of each device
        local seen, buttons, dialog = {}, {}, nil
        for _i, record in ipairs(body.history or {}) do
            if not seen[record.device_id] and #buttons < 10 then
                seen[record.device_id] = true
                local label = self:describe(record)
                if record.device_id == self.device_id then label = T(_("%1 (this device)"), label) end
                if record.manual then label = label .. " ✎" end
                table.insert(buttons, { {
                    text = label,
                    align = "left",
                    callback = function()
                        UIManager:close(dialog)
                        self:goTo(record)
                    end,
                } })
            end
        end
        if #buttons == 0 then
            UIManager:show(InfoMessage:new{ text = _("No device has synced this book yet."), timeout = 3 })
            return
        end
        dialog = ButtonDialog:new{ title = _("Go to where a device left this book"), buttons = buttons }
        UIManager:show(dialog)
    end)
end

local function choiceMenu(self, key, default, choices)
    local items = {}
    for _i, choice in ipairs(choices) do
        table.insert(items, {
            text = choice[2],
            checked_func = function() return (self.state:readSetting(key) or default) == choice[1] end,
            radio = true,
            callback = function()
                self.state:saveSetting(key, choice[1])
                self.state:flush()
            end,
        })
    end
    return items
end

function OPDShelf:addToMainMenu(menu_items)
    self:addStartWith(menu_items)
    menu_items.opdshelf = {
        text = _("OPDShelf"),
        sorting_hint = "tools",
        sub_item_table_func = function() return self:menuItems() end,
    }
end

function OPDShelf:menuItems()
    local in_book = self:hasBook()
    return {
        {
            text = _("Home"),
            callback = function() self:showHome() end,
        },
        {
            text = _("Library"),
            callback = function() self:showLibrary() end,
        },
        {
            text = _("Search the library"),
            callback = function() self:showLibrary(true) end,
        },
        {
            text = _("Show the home screen at start"),
            help_text = _("Opens OPDShelf's home screen instead of the file browser when KOReader starts and after closing a book, and the file browser's home button goes there."),
            checked_func = function() return self:launcherEnabled() end,
            callback = function() self:setLauncher(not self:launcherEnabled()) end,
            separator = true,
        },
        {
            text = _("Sync this book now"),
            enabled_func = function() return in_book end,
            callback = function() self:onOPDShelfSync() end,
        },
        {
            text = _("Go to where another device left it"),
            enabled_func = function() return in_book and self:ready() end,
            callback = function() self:showPositions() end,
            separator = true,
        },
        {
            text = _("Live sync while reading"),
            help_text = _("Keeps a connection to the server while a book is open, so reading done on other devices arrives at once. Positions are always sent after each page turn."),
            checked_func = function() return self:liveEnabled() end,
            callback = function()
                self.state:saveSetting("live", not self:liveEnabled())
                self.state:flush()
                if self:liveEnabled() then self:startStream() else self:stopStream() end
            end,
        },
        {
            text = _("When another device is further ahead"),
            sub_item_table = choiceMenu(self, "forward", "jump", {
                { "jump", _("Go there") },
                { "ask", _("Ask") },
                { "ignore", _("Stay here") },
            }),
        },
        {
            text = _("When another device is behind"),
            sub_item_table = choiceMenu(self, "backward", "ask", {
                { "ask", _("Ask") },
                { "jump", _("Go there") },
                { "ignore", _("Stay here") },
            }),
            separator = true,
        },
        {
            text_func = function()
                local server = self.state:readSetting("server")
                return server and T(_("Server: %1"), server) or _("Server and account")
            end,
            keep_menu_open = true,
            callback = function(touchmenu_instance)
                self:editAccount(function()
                    if touchmenu_instance then touchmenu_instance:updateItems() end
                end)
            end,
        },
        {
            text_func = function() return T(_("Device name: %1"), self:deviceName()) end,
            keep_menu_open = true,
            callback = function(touchmenu_instance)
                local dialog
                dialog = InputDialog:new{
                    title = _("Name of this device on other devices"),
                    input = self:deviceName(),
                    buttons = { {
                        { text = _("Cancel"), id = "close", callback = function() UIManager:close(dialog) end },
                        { text = _("Save"), is_enter_default = true, callback = function()
                            local name = util.trim(dialog:getInputText() or "")
                            self.state:saveSetting("device_name", name ~= "" and name or nil)
                            self.state:flush()
                            UIManager:close(dialog)
                            if touchmenu_instance then touchmenu_instance:updateItems() end
                        end },
                    } },
                }
                UIManager:show(dialog)
                dialog:onShowKeyboard()
            end,
        },
        {
            text_func = function() return T(_("Download folder: %1"), self:downloadDir()) end,
            keep_menu_open = true,
            callback = function(touchmenu_instance)
                self:chooseDownloadDir(function()
                    if touchmenu_instance then touchmenu_instance:updateItems() end
                end)
            end,
        },
    }
end

return OPDShelf

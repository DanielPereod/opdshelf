--[[--
OPDShelf for KOReader.

Adds "Set as last position" to the menu shown when selecting text. The start
of the selection is sent to the OPDShelf server as this book's position, so
every device (KOReader, the web reader, the app) opens the book on that exact
sentence instead of at the top of whatever page it falls on.

It works alongside KOReader's own Progress sync plugin and uses its server and
account: set Progress sync's custom server to https://<your-server>/kosync and
log in there first.

@module koplugin.opdshelf
--]]

local ConfirmBox = require("ui/widget/confirmbox")
local DataStorage = require("datastorage")
local Device = require("device")
local Event = require("ui/event")
local InfoMessage = require("ui/widget/infomessage")
local JSON = require("json")
local LuaSettings = require("luasettings")
local NetworkMgr = require("ui/network/manager")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local http = require("socket.http")
local logger = require("logger")
local ltn12 = require("ltn12")
local socket = require("socket")
local socketutil = require("socketutil")
local util = require("util")
local T = require("ffi/util").template
local _ = require("gettext")

-- Words of text saved before and after a position, to find it again if the
-- xpointer does not lead to the same words in another reader
local CONTEXT_WORDS = 8
local HIGHLIGHT_SECONDS = 6

local OPDShelf = WidgetContainer:extend{
    name = "opdshelf",
    is_doc_only = true,
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

function OPDShelf:init()
    self.state = LuaSettings:open(DataStorage:getSettingsDir() .. "/opdshelf.lua")
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

-- The server and account configured in Progress sync
function OPDShelf:account()
    local settings = self.ui.kosync and self.ui.kosync.settings
    if not settings then
        local file = LuaSettings:open(DataStorage:getSettingsDir() .. "/kosync.lua")
        settings = file:readSetting("settings") or {}
    end
    if not (settings.custom_server and settings.username and settings.userkey) then
        return nil
    end
    return {
        server = settings.custom_server:gsub("/+$", ""),
        username = settings.username,
        userkey = settings.userkey,
        device = settings.kosync_hostname or Device.model,
        auto_sync = settings.auto_sync,
    }
end

function OPDShelf:digest()
    if self.ui.kosync and self.ui.kosync.getDocumentDigest then
        return self.ui.kosync:getDocumentDigest()
    end
    return self.ui.doc_settings:readSetting("partial_md5_checksum")
end

-- Returns the HTTP status (a number) and the decoded JSON body, or nil and an error
function OPDShelf:request(account, method, path, body)
    local sink = {}
    local headers = {
        ["Accept"] = "application/vnd.koreader.v1+json",
        ["x-auth-user"] = account.username,
        ["x-auth-key"] = account.userkey,
    }
    local source
    if body then
        local data = JSON.encode(body)
        headers["Content-Type"] = "application/json"
        headers["Content-Length"] = tostring(#data)
        source = ltn12.source.string(data)
    end
    socketutil:set_timeout(5, 10)
    local code, _headers, status = socket.skip(1, http.request{
        url = account.server .. path,
        method = method,
        headers = headers,
        source = source,
        sink = ltn12.sink.table(sink),
    })
    socketutil:reset_timeout()
    if type(code) ~= "number" then
        logger.warn("OPDShelf: request failed:", status or code)
        return nil, status or code
    end
    local ok, result = pcall(JSON.decode, table.concat(sink))
    return code, ok and result or nil
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

function OPDShelf:setLastPosition(selected)
    local xp = selected and selected.pos0
    if type(xp) ~= "string" then return end
    local account = self:account()
    if not account then
        UIManager:show(InfoMessage:new{
            text = _("Set up Progress sync first: use your OPDShelf server followed by /kosync as its custom sync server, and log in."),
        })
        return
    end
    local document = self:digest()
    if not document then return end

    local context = self:contextAt(xp)
    context.selected = utf8Head(util.cleanupSelectedText(selected.text or ""), 200)
    context.before = utf8Head(context.before, 400)
    context.after = utf8Head(context.after, 400)

    -- Until this page is left, Progress sync sends this point instead of the top of the page
    self:setMark(xp)

    local pending = self.state:readSetting("pending", {})
    pending[document] = {
        document = document,
        progress = xp,
        percentage = self:percentageAt(xp),
        device = account.device,
        device_id = G_reader_settings:readSetting("device_id"),
        manual = true,
        timestamp = os.time(),
        context = context,
    }
    self.state:saveSetting("pending", pending)
    self.state:flush()

    if NetworkMgr:isOnline() then
        self:flush(true)
    else
        UIManager:show(InfoMessage:new{
            text = _("Last position saved. It will be sent when the device is online."),
            timeout = 3,
        })
    end
end

-- Sends the positions set while offline. Each book keeps only its latest one.
function OPDShelf:flush(interactive)
    local pending = self.state:readSetting("pending", {})
    if next(pending) == nil then return end
    local account = self:account()
    if not account then return end
    local sent, failed, rejected = 0, false, false
    for document, item in pairs(pending) do
        local code = self:request(account, "PUT", "/syncs/progress", item)
        if code == 200 then
            pending[document] = nil
            sent = sent + 1
        elseif code == 401 then
            rejected = true
        elseif code and code >= 400 and code < 500 then
            -- The server will never accept it
            logger.warn("OPDShelf: position rejected with", code)
            pending[document] = nil
        else
            failed = true
        end
    end
    self.state:saveSetting("pending", pending)
    self.state:flush()
    if not interactive then return end
    local text
    if rejected then
        text = _("The server did not accept the Progress sync login. Log in again in Progress sync.")
    elseif failed then
        text = _("Cannot reach the server. The position will be sent later.")
    elseif sent > 0 then
        text = _("Last position set. Other devices will open the book here.")
    end
    if text then
        UIManager:show(InfoMessage:new{ text = text, timeout = 3 })
    end
end

function OPDShelf:setMark(xp)
    self.mark = xp
    if xp then
        self.ui.doc_settings:saveSetting("opdshelf_mark", xp)
    else
        self.ui.doc_settings:delSetting("opdshelf_mark")
    end
end

-- While the marked point is on screen, Progress sync pushes it as the
-- position, so closing the book or putting the device to sleep does not
-- replace it with the top of the page.
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

function OPDShelf:onReaderReady()
    if self.ui.document.info.has_pages then return end
    self.mark = self.ui.doc_settings:readSetting("opdshelf_mark")
    self:patchProgressSync()
    self.check_task = function() self:checkServerMark() end
    -- After Progress sync had its chance to move to the latest position
    UIManager:scheduleIn(4, self.check_task)
end

-- When the book's last position was set by hand on another device, offer to go there and show it
function OPDShelf:checkServerMark()
    if self.closed or not NetworkMgr:isOnline() then return end
    local account = self:account()
    local document = self:digest()
    if not account or not document then return end
    local code, body = self:request(account, "GET", "/syncs/progress/" .. document)
    if code ~= 200 or type(body) ~= "table" or body.manual ~= true then return end
    local xp = body.progress
    if type(xp) ~= "string" or xp:sub(1, 1) ~= "/" then return end
    if body.device_id == G_reader_settings:readSetting("device_id") then return end

    self:setMark(xp)
    local selected = type(body.context) == "table" and body.context.selected or ""
    self.pending_highlight = { xp = xp, selected = selected }
    if self.ui.document:isXPointerInCurrentPage(xp) then
        self:showMark()
        return
    end
    UIManager:show(ConfirmBox:new{
        text = T(_("A point to continue this book from was set on %1. Go there?"), body.device or _("another device")),
        ok_text = _("Go"),
        ok_callback = function()
            self.ui:handleEvent(Event:new("GotoXPointer", xp, xp))
        end,
        cancel_callback = function()
            self.pending_highlight = nil
        end,
    })
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

function OPDShelf:onMoved()
    if self.ui.document.info.has_pages then return end
    if self.pending_highlight and self.ui.document:isXPointerInCurrentPage(self.pending_highlight.xp) then
        self:setMark(self.pending_highlight.xp)
        -- Wait for the page to be drawn
        UIManager:nextTick(function() self:showMark() end)
    elseif self.mark and not self.ui.document:isXPointerInCurrentPage(self.mark) then
        -- Reading went on: Progress sync sends the page again
        self:setMark(nil)
    end
end

function OPDShelf:onPageUpdate()
    self:onMoved()
end

function OPDShelf:onPosUpdate()
    self:onMoved()
end

function OPDShelf:onNetworkConnected()
    UIManager:scheduleIn(1, function() self:flush(false) end)
end

function OPDShelf:onCloseDocument()
    self.closed = true
    if self.check_task then UIManager:unschedule(self.check_task) end
    if self.clear_task then UIManager:unschedule(self.clear_task) end
end

return OPDShelf

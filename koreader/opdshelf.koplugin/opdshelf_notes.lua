--[[--
Highlights and notes shared with the OPDShelf server: turns KOReader's
annotations into the server's and back, and works out what changed.

Kept apart from the reader so it can be tested on its own.

Each highlight gets an id (`opdshelf_id`, saved in the annotation itself) so
every device changes or deletes the same one. The book remembers what it last
agreed with the server (`snapshot`: id -> signature and time); a highlight
that differs from it was changed here and is sent, one that is missing was
deleted here. That way changes made offline, or while the plugin could not
reach the server, are sent the next time the book syncs.

@module opdshelf_notes
--]]

local Notes = {}

-- Seconds to add to this device's clock to get the server's. Times kept for the highlights
-- (`opdshelf_updated`, the snapshot) are in the server's clock, like everything the server
-- answers, so a Kobo whose clock is off still compares them right; they are sent back in the
-- device's clock together with `sent_at`, which is how the server corrects them.
Notes.offset = 0

function Notes.setOffset(seconds)
    Notes.offset = type(seconds) == "number" and seconds or 0
end

local COLORS = {
    yellow = true, orange = true, red = true, purple = true, blue = true,
    cyan = true, green = true, olive = true, gray = true,
}
local STYLES = { lighten = true, underscore = true, strikeout = true, invert = true }

--- "2024-05-01 12:30:00" (KOReader's local time) as Unix seconds.
function Notes.parseTime(s)
    if type(s) ~= "string" then return nil end
    local y, m, d, H, M, S = s:match("^(%d+)-(%d+)-(%d+) (%d+):(%d+):(%d+)")
    if not y then return nil end
    return os.time{ year = tonumber(y), month = tonumber(m), day = tonumber(d),
        hour = tonumber(H), min = tonumber(M), sec = tonumber(S) }
end

function Notes.formatTime(t)
    return os.date("%Y-%m-%d %H:%M:%S", t)
end

--- Whether an annotation is a highlight (bookmarks of a page have no text range).
function Notes.isHighlight(item)
    return type(item) == "table" and item.pos0 ~= nil and item.pos1 ~= nil
end

local function position(pos)
    if type(pos) == "string" then return pos end
    if type(pos) == "table" and pos.page then return tostring(pos.page) end
    return ""
end

local function blank(s)
    return s == nil or s == "" or (type(s) == "string" and s:match("^%s*$") ~= nil)
end

--- What a highlight looks like: what other devices care about.
function Notes.signature(a)
    return table.concat({
        position(a.pos0), position(a.pos1),
        a.color or "yellow", a.drawer or a.style or "lighten",
        blank(a.note) and "" or a.note,
    }, "\1")
end

--- A new id for a highlight made on this device.
function Notes.newId(device_id, item, seed)
    local base = (device_id or "k"):gsub("[^%w]", ""):sub(1, 8)
    local stamp = (Notes.parseTime(item.datetime) or os.time())
    local salt = (seed or math.random(0, 0xffffff))
    return string.format("k%s%x%06x", base, stamp, salt % 0x1000000)
end

--- When a highlight was last changed here, in the server's clock.
function Notes.localTime(item)
    return item.opdshelf_updated
        or ((Notes.parseTime(item.datetime_updated) or Notes.parseTime(item.datetime) or os.time()) + Notes.offset)
end

--[[--
The server's form of a KOReader annotation.
@param item the annotation
@param extra { percentage, page, device, device_id }
--]]
function Notes.toServer(item, extra)
    extra = extra or {}
    local pages = type(item.pos0) == "table"
    local style = STYLES[item.drawer or ""] and item.drawer or "lighten"
    local color = COLORS[item.color or ""] and item.color or "yellow"
    local server = item.opdshelf_server or {}
    return {
        id = item.opdshelf_id,
        text = item.text or "",
        note = (not blank(item.note)) and item.note or "",
        color = color,
        style = style,
        pos0 = position(item.pos0),
        pos1 = position(item.pos1),
        chapter = item.chapter or "",
        page = pages and item.pos0.page or (type(item.pageno) == "number" and item.pageno or nil),
        percentage = extra.percentage or server.percentage or 0,
        -- Kept from the server, so the web reader still finds it exactly
        cfi = server.cfi,
        context = server.context,
        created = Notes.parseTime(item.datetime) or Notes.localTime(item),
        updated = Notes.localTime(item) - Notes.offset,
        device = extra.device,
        device_id = extra.device_id,
    }
end

--- KOReader's annotation for a highlight from the server (reflowable books).
function Notes.fromServer(a)
    return {
        datetime = Notes.formatTime(a.created or a.updated or os.time()),
        datetime_updated = a.updated and Notes.formatTime(a.updated) or nil,
        text = a.text or "",
        note = (not blank(a.note)) and a.note or nil,
        color = COLORS[a.color or ""] and a.color or "yellow",
        drawer = STYLES[a.style or ""] and a.style or "lighten",
        chapter = (a.chapter ~= "" and a.chapter) or nil,
        pos0 = a.pos0,
        pos1 = a.pos1,
        page = a.pos0,
        opdshelf_id = a.id,
        opdshelf_updated = a.updated,
        opdshelf_server = { cfi = a.cfi, context = a.context, percentage = a.percentage },
    }
end

--[[--
Records when highlights changed here, since the last agreement, and when ones
were deleted. Call it whenever KOReader says the annotations changed.
@param items KOReader's annotations
@param snapshot id -> { sig, updated } (changed in place)
@param now Unix seconds
@param newId function(item) for highlights without one yet
@return whether anything changed
--]]
function Notes.track(items, snapshot, now, newId)
    now = now + Notes.offset
    local changed = false
    local present = {}
    for _, item in ipairs(items) do
        if Notes.isHighlight(item) then
            if not item.opdshelf_id then
                item.opdshelf_id = newId(item)
            end
            present[item.opdshelf_id] = true
            local kept = snapshot[item.opdshelf_id]
            local sig = Notes.signature(item)
            if (not kept or kept.deleted or kept.sig ~= sig) and item.opdshelf_sig ~= sig then
                item.opdshelf_sig = sig
                item.opdshelf_updated = math.max(now, (kept and kept.updated or 0))
                changed = true
            end
        end
    end
    for id, kept in pairs(snapshot) do
        -- (An entry marked `missing` is a highlight of another device that is not in this book)
        if not present[id] and not kept.deleted and not kept.missing then
            kept.deleted = true
            kept.deleted_at = math.max(now, kept.updated or 0)
            changed = true
        end
    end
    return changed
end

--[[--
What to send: highlights changed here and deletions made here.
@return list of server annotations
--]]
function Notes.outgoing(items, snapshot, extra_for)
    local out = {}
    for _, item in ipairs(items) do
        if Notes.isHighlight(item) and item.opdshelf_id then
            local kept = snapshot[item.opdshelf_id]
            if not kept or kept.deleted or kept.sig ~= Notes.signature(item) then
                out[#out+1] = Notes.toServer(item, extra_for(item))
            end
        end
    end
    for id, kept in pairs(snapshot) do
        if kept.deleted and not kept.sent then
            local extra = extra_for(nil)
            out[#out+1] = { id = id, deleted = true, updated = (kept.deleted_at or kept.updated) - Notes.offset,
                device = extra.device, device_id = extra.device_id }
        end
    end
    return out
end

--[[--
Works out what to do with the server's highlights of the book.
@param items KOReader's annotations
@param list the server's highlights (tombstones included)
@param snapshot id -> { sig, updated } (changed in place to the new agreement)
@param complete whether `list` is all the server has for the book (not just what changed): a
  highlight agreed on that the server no longer has (another server, a lost file) is forgotten
  so that it is sent again
@param force id -> true for the highlights the server said it kept its own version of: its version
  wins whatever the times say
@return actions: { add = {server…}, update = {{item, server}…}, remove = {item…} }
--]]
function Notes.merge(items, list, snapshot, complete, force)
    force = force or {}
    local by_id = {}
    for _, item in ipairs(items) do
        if Notes.isHighlight(item) and item.opdshelf_id then by_id[item.opdshelf_id] = item end
    end
    local actions = { add = {}, update = {}, remove = {} }
    if complete then
        local on_server = {}
        for _, s in ipairs(list or {}) do
            if type(s) == "table" and type(s.id) == "string" then on_server[s.id] = true end
        end
        for id, kept in pairs(snapshot) do
            if by_id[id] and not kept.deleted and not kept.missing and not on_server[id] then
                snapshot[id] = nil
            end
        end
    end
    for _, s in ipairs(list or {}) do
        if type(s) == "table" and type(s.id) == "string" then
            local item = by_id[s.id]
            local kept = snapshot[s.id]
            local sig = Notes.signature(s)
            local changed_here = item and (not kept or kept.deleted or kept.sig ~= Notes.signature(item))
            local here = item and (changed_here and Notes.localTime(item) or (kept and kept.updated or 0)) or 0
            if force[s.id] then here = -1 end
            if s.deleted then
                if item and (not changed_here or here <= (s.updated or 0)) then
                    actions.remove[#actions.remove+1] = item
                    snapshot[s.id] = nil
                elseif not item then
                    snapshot[s.id] = nil
                end
            elseif not item then
                if kept and kept.missing and kept.sig == sig then
                    -- Already tried: its text is not in this copy of the book
                elseif kept and kept.deleted and not force[s.id] and (kept.deleted_at or 0) >= (s.updated or 0) then
                    -- Deleted here after its last change elsewhere: the deletion goes out
                    kept.sent = nil
                elseif type(s.pos0) == "string" and s.pos0:sub(1, 1) == "/" then
                    actions.add[#actions.add+1] = s
                    snapshot[s.id] = { sig = sig, updated = s.updated }
                end
            elseif sig ~= Notes.signature(item) then
                if (s.updated or 0) > here then
                    actions.update[#actions.update+1] = { item, s }
                    snapshot[s.id] = { sig = sig, updated = s.updated }
                end
                -- Otherwise the change made here is newer and goes out
            else
                snapshot[s.id] = { sig = sig, updated = s.updated }
                item.opdshelf_server = { cfi = s.cfi, context = s.context, percentage = s.percentage }
            end
        end
    end
    return actions
end

--- Marks what the server took (its answer's highlights) as agreed.
function Notes.agree(snapshot, sent, answer)
    local by_id = {}
    for _, s in ipairs(answer or {}) do
        if type(s) == "table" and s.id then by_id[s.id] = s end
    end
    for _, a in ipairs(sent) do
        local s = by_id[a.id]
        if a.deleted then
            snapshot[a.id] = nil
        elseif s and not s.deleted then
            snapshot[a.id] = { sig = Notes.signature(s), updated = s.updated }
        end
    end
end

return Notes

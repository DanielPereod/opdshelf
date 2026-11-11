--[[--
Decides what to do when the server reports a book's position.

Kept apart from the reader so it can be tested on its own. Times are the
server's (positions come back from it with corrected timestamps), so devices
with a wrong clock still compare fairly.

@module opdshelf_sync
--]]

local Sync = {}

--- Seconds after a page turn during which the reader counts as reading right now.
Sync.ACTIVE_SECONDS = 45

--[[--
What to do with the server's position of the open book.

@param o table
  remote: the server's current position (or nil)
  me: this device's id
  synced: the last server position this device agreed with or dismissed
  moved: whether reading went on here since then (also offline, in earlier sessions)
  showing: whether the remote position is on the page shown now
  ahead: whether the remote position is further in the book than this one
  reason: "open", "live" (pushed by the server while reading), "rejected"
          (the server kept a newer position instead of ours) or "manual" (asked from the menu)
  idle: seconds since the last page turn here
  forward, backward: what to do with a newer position further ahead / behind:
          "jump", "ask" or "ignore"
@return string one of:
  "none": nothing to do
  "push": this device is newer, send its position
  "jump": go to the remote position
  "ask": offer to go to the remote position
  "conflict": both devices read since they last agreed: let the reader choose
--]]
function Sync.decide(o)
    local remote = o.remote
    if type(remote) ~= "table" or type(remote.progress) ~= "string" or remote.progress == "" then
        return o.moved and "push" or "none"
    end
    -- Our own position coming back
    if remote.device_id == o.me then
        return "none"
    end
    if o.showing then
        return "none"
    end
    local synced = o.synced
    local new = not synced
        or (remote.timestamp or 0) > (synced.timestamp or 0)
        or ((remote.timestamp or 0) == (synced.timestamp or 0)
            and (remote.device_id ~= synced.device_id or remote.progress ~= synced.progress))
    if not new and o.reason ~= "manual" then
        -- Already seen and left aside: our next page turn will win
        return o.moved and "push" or "none"
    end
    if o.reason == "manual" then
        return "ask"
    end
    local reading_now = (o.idle or math.huge) < Sync.ACTIVE_SECONDS
    if o.moved and not (o.reason == "live" and reading_now) then
        -- Read here (maybe offline) and there since the last agreement
        return "conflict"
    end
    if remote.manual or reading_now then
        -- Never move the page under the reader's eyes
        return "ask"
    end
    local setting = o.ahead and (o.forward or "jump") or (o.backward or "ask")
    if setting == "ignore" then
        return "none"
    end
    return setting
end

--- "just now", "5 min ago", "3 h ago", "2 days ago" between two server times.
-- `_` translates and `T` fills %1 in a template, as KOReader's gettext and ffi/util.template.
function Sync.ago(timestamp, now, _, T)
    local seconds = math.max(0, (now or os.time()) - (timestamp or 0))
    if seconds < 60 then
        return _("just now")
    elseif seconds < 3600 then
        return T(_("%1 min ago"), math.floor(seconds / 60))
    elseif seconds < 86400 then
        return T(_("%1 h ago"), math.floor(seconds / 3600))
    end
    return T(_("%1 days ago"), math.floor(seconds / 86400))
end

return Sync

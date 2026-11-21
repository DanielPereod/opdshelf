--[[--
What KOReader's own Statistics plugin knows of the reading on this device:
the seconds read today and when each book was last read. It keeps every page
read, with when it started and for how long, in statistics.sqlite3 in the
settings folder; this only reads it, the way the Statistics plugin opens it.

The server counts reading from the positions devices send, which it gets on
page turns: the first and the last page of a short session never reach it.
So the home screen shows the most of the two for today, and a day read here
keeps the streak going even before the server has counted it.

@module opdshelf_localstats
--]]

local LocalStats = {}

--- A day with this much reading keeps a streak going, as on the server.
LocalStats.STREAK_SECONDS = 60

--- Where the Statistics plugin keeps its database.
function LocalStats.path()
    local ok, DataStorage = pcall(require, "datastorage")
    if not ok or type(DataStorage) ~= "table" or not DataStorage.getSettingsDir then return nil end
    return DataStorage:getSettingsDir() .. "/statistics.sqlite3"
end

--- The start of today on this device's clock.
function LocalStats.dayStart(now)
    local t = os.date("*t", now or os.time())
    return os.time{ year = t.year, month = t.month, day = t.day, hour = 0, min = 0, sec = 0 }
end

--[[--
Reads the database: `{ today = seconds read since `day_start`, last = { [md5] =
when that book was last read } }` for the books whose partial MD5 is in
`md5s`, or nil without a database (the Statistics plugin is off, or this is
not KOReader). Never fails.
--]]
function LocalStats.read(day_start, md5s, path)
    path = path or LocalStats.path()
    if not path then return nil end
    local file = io.open(path, "rb")
    if not file then return nil end
    file:close()
    local ok, SQ3 = pcall(require, "lua-ljsqlite3/init")
    if not ok or type(SQ3) ~= "table" or not SQ3.open then return nil end
    local opened, conn = pcall(SQ3.open, path, "ro")
    if not opened or not conn then return nil end
    local out = { last = {} }
    pcall(function()
        local seconds = conn:rowexec(string.format(
            "SELECT sum(duration) FROM page_stat_data WHERE start_time >= %d", math.floor(day_start)))
        out.today = tonumber(seconds) or 0
    end)
    -- Only document ids made of hex digits go into the query
    local ids = {}
    for _i, md5 in ipairs(md5s or {}) do
        if type(md5) == "string" and md5:match("^%x+$") then ids[#ids + 1] = "'" .. md5 .. "'" end
    end
    if #ids > 0 then
        pcall(function()
            local result = conn:exec(string.format(
                "SELECT book.md5, max(page_stat_data.start_time + page_stat_data.duration) FROM book "
                .. "JOIN page_stat_data ON page_stat_data.id_book = book.id WHERE book.md5 IN (%s) GROUP BY book.md5",
                table.concat(ids, ",")))
            if type(result) == "table" and type(result[1]) == "table" then
                for i, md5 in ipairs(result[1]) do
                    local at = tonumber(result[2] and result[2][i])
                    if at then out.last[tostring(md5)] = at end
                end
            end
        end)
    end
    pcall(conn.close, conn)
    return out
end

--[[--
The home screen's statistics with this device's own reading today: `stats`
is what the server said (`date`, `today_seconds`, `streak`, as kept),
`here` the seconds read today on this device (nil when not known) and
`today` the date as os.date("%Y-%m-%d"). Today is the most of the two (the
server's counts every device, this one counts what the server did not get);
a day read here keeps the streak, so it is one more when the server has not
counted today yet; a streak the server gave before yesterday is over by now.
Returns a new table; `stats` stays the server's.
--]]
function LocalStats.combine(stats, here, today, yesterday)
    local out = {}
    for k, v in pairs(type(stats) == "table" and stats or {}) do out[k] = v end
    -- A streak the server gave before yesterday is over, even if nothing was read here today
    -- (offline for days, the home screen still showed "12 days")
    if yesterday and type(out.date) == "string" and out.date ~= today and out.date ~= yesterday and tonumber(out.streak) then
        out.streak = 0
    end
    here = tonumber(here)
    if not here or here <= 0 then return out end
    local server_today = out.date == today and tonumber(out.today_seconds) or nil
    if here >= LocalStats.STREAK_SECONDS and (server_today or 0) < LocalStats.STREAK_SECONDS then
        local alive = out.date == today or (yesterday and out.date == yesterday)
        out.streak = (alive and tonumber(out.streak) or 0) + 1
    end
    out.date = today
    out.today_seconds = math.max(server_today or 0, here)
    return out
end

return LocalStats

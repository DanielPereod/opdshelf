--[[--
Updates of the plugin from its OPDShelf server.

The server says which version its channel offers at /apps/versions.json, and
serves the plugin at /apps/opdshelf.koplugin.zip. The release workflow stamps
the version in _meta.lua as a number (the commit's date and time, like
202611141233); a copy without it (a development checkout) does not update.

Installing unpacks the download next to the plugin, then swaps the folders
with renames, so a failure at any step leaves the installed plugin as it was.

@module opdshelf_update
--]]

local Update = {}

Update.URL = "/apps/opdshelf.koplugin.zip"

--- The version in the plugin folder's _meta.lua, or nil.
function Update.localVersion(dir)
    local file = dir and io.open(dir .. "/_meta.lua", "r")
    if not file then return nil end
    local text = file:read("*a")
    file:close()
    return tonumber(text:match("version%s*=%s*\"?(%d+)"))
end

--- 202611141233 → "2026-11-14 12:33"
function Update.describe(version)
    local s = tostring(version)
    local y, m, d, hh, mm = s:match("^(%d%d%d%d)(%d%d)(%d%d)(%d%d)(%d%d)$")
    if y then return string.format("%s-%s-%s %s:%s", y, m, d, hh, mm) end
    return s
end

--- The version the server offers. Returns it, or nil and why.
function Update.latest(client)
    local code, body = client:json_request("GET", "/apps/versions.json")
    if code ~= 200 or type(body) ~= "table" then
        return nil, code and ("HTTP " .. code) or tostring(body)
    end
    local version = type(body.koreader) == "table" and tonumber(body.koreader.version)
    if not version then return nil, "no version" end
    return version
end

--[[--
Downloads the plugin and puts it in place of the one in `dir`.
@param fs table: exists(path), mkdir(path), purge(dir), unpack(zip, dir) → ok, err
@return true, or false and why
--]]
function Update.install(client, dir, fs)
    dir = dir:gsub("/+$", "")
    local parent = dir:match("^(.*)/[^/]+$")
    if not parent then return false, "unknown plugin folder" end
    local zip = parent .. "/opdshelf-update.zip"
    local tmp = parent .. "/opdshelf-update"
    local old = dir .. ".old"
    local ok, err = client:download(Update.URL, zip)
    if not ok then return false, err end
    fs.purge(tmp)
    fs.mkdir(tmp)
    ok, err = fs.unpack(zip, tmp)
    os.remove(zip)
    local new
    if fs.exists(tmp .. "/opdshelf.koplugin/main.lua") then
        new = tmp .. "/opdshelf.koplugin"
    elseif fs.exists(tmp .. "/main.lua") then
        new = tmp
    end
    if not ok or not new then
        fs.purge(tmp)
        return false, err or "the download is not the plugin"
    end
    -- The account is already set up here
    os.remove(new .. "/opdshelf_setup.json")
    fs.purge(old)
    local moved, move_err = os.rename(dir, old)
    if not moved then
        fs.purge(tmp)
        return false, move_err
    end
    local placed, place_err = os.rename(new, dir)
    if not placed then
        os.rename(old, dir)
        fs.purge(tmp)
        return false, place_err
    end
    fs.purge(old)
    fs.purge(tmp)
    return true
end

--- The file functions, with KOReader's libraries.
function Update.koreaderFs()
    local lfs = require("libs/libkoreader-lfs")
    local ffiutil = require("ffi/util")
    local Device = require("device")
    return {
        exists = function(path) return lfs.attributes(path, "mode") ~= nil end,
        mkdir = function(path) return lfs.mkdir(path) end,
        purge = function(path)
            if lfs.attributes(path, "mode") == "directory" then pcall(ffiutil.purgeDir, path) end
        end,
        unpack = function(zip, to)
            local ok, result, err = pcall(Device.unpackArchive, Device, zip, to)
            if not ok then return false, tostring(result) end
            if not result then return false, err or "cannot unzip" end
            return true
        end,
    }
end

return Update

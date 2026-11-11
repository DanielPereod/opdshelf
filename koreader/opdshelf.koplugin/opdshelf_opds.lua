--[[--
Reads OPDS 1.x catalog feeds (Atom) into plain tables.

Only what the library browser needs: entries with their title, authors,
series, summary and links (covers, downloads, subsections), and the feed's
own links (next page, search, up).

@module opdshelf_opds
--]]

local OPDS = {}

local ACQUISITION = "http://opds-spec.org/acquisition"
local IMAGE = "http://opds-spec.org/image"
local THUMBNAIL = "http://opds-spec.org/image/thumbnail"

local function utf8char(code)
    if code < 0x80 then
        return string.char(code)
    elseif code < 0x800 then
        return string.char(0xC0 + math.floor(code / 0x40), 0x80 + code % 0x40)
    elseif code < 0x10000 then
        return string.char(0xE0 + math.floor(code / 0x1000), 0x80 + math.floor(code / 0x40) % 0x40, 0x80 + code % 0x40)
    elseif code < 0x110000 then
        return string.char(0xF0 + math.floor(code / 0x40000), 0x80 + math.floor(code / 0x1000) % 0x40,
            0x80 + math.floor(code / 0x40) % 0x40, 0x80 + code % 0x40)
    end
    return "?"
end

local named = { amp = "&", lt = "<", gt = ">", quot = '"', apos = "'", nbsp = "\u{00A0}" }

--- Decodes XML character references and entities.
function OPDS.decode(text)
    if not text then return nil end
    text = text:gsub("<!%[CDATA%[(.-)%]%]>", function(cdata)
        -- Protect CDATA from entity decoding
        return (cdata:gsub("&", "\0"))
    end)
    text = text:gsub("&(#?[xX]?)(%w+);", function(kind, value)
        if kind == "" then
            return named[value] or ("&" .. value .. ";")
        end
        local code = kind == "#" and tonumber(value) or tonumber(value, 16)
        return code and utf8char(code) or ""
    end)
    return (text:gsub("%z", "&"))
end

local function attributes(tag)
    local attrs = {}
    for name, quote, value in tag:gmatch("([%w:_%-]+)%s*=%s*([\"'])(.-)%2") do -- luacheck: no unused
        attrs[name] = OPDS.decode(value)
    end
    return attrs
end

-- Text of the first <name>…</name> (with any namespace prefix) in xml
local function element(xml, name)
    local value = xml:match("<" .. name .. "%f[%s>][^>]*>(.-)</" .. name .. ">")
    if not value then return nil end
    -- Drop markup inside (xhtml content) and squeeze spaces
    value = OPDS.decode(value:gsub("<[^>]+>", " "))
    value = value:gsub("%s+", " "):gsub("^ ", ""):gsub(" $", "")
    if value == "" then return nil end
    return value
end

local function links(xml)
    local list = {}
    for tag in xml:gmatch("<link%f[%s/>][^>]*>") do
        list[#list + 1] = attributes(tag)
    end
    return list
end

--- Makes `href` absolute against the URL of the feed it came from.
function OPDS.absolute(base, href)
    if not href or href == "" then return nil end
    if href:match("^%a[%w+.-]*:") then return href end
    local scheme, authority, path = base:match("^(%a[%w+.-]*://)([^/?#]*)([^?#]*)")
    if not scheme then return href end
    if href:sub(1, 2) == "//" then
        return scheme:gsub("//$", "") .. href
    end
    if href:sub(1, 1) == "/" then
        return scheme .. authority .. href
    end
    if href:sub(1, 1) == "?" then
        return scheme .. authority .. path .. href
    end
    local dir = path:match("^(.*/)") or "/"
    return scheme .. authority .. dir .. href
end

local function isAcquisition(rel)
    return rel and (rel == ACQUISITION or rel:sub(1, #ACQUISITION + 1) == ACQUISITION .. "/")
end

local function parseEntry(xml, base)
    local entry = {
        title = element(xml, "title") or "",
        id = element(xml, "id"),
        authors = {},
        summary = element(xml, "summary") or element(xml, "content"),
        language = element(xml, "dc:language"),
        publisher = element(xml, "dc:publisher"),
        issued = element(xml, "dc:issued"),
        series = element(xml, "calibre:series"),
        series_index = element(xml, "calibre:series_index"),
        categories = {},
        acquisitions = {},
    }
    for author in xml:gmatch("<author%f[%s>][^>]*>(.-)</author>") do
        local name = element(author, "name")
        if name then entry.authors[#entry.authors + 1] = name end
    end
    for tag in xml:gmatch("<category%f[%s/>][^>]*>") do
        local attrs = attributes(tag)
        local label = attrs.label or attrs.term
        if label and label ~= "" then entry.categories[#entry.categories + 1] = label end
    end
    for _, link in ipairs(links(xml)) do
        local href = OPDS.absolute(base, link.href)
        local rel = link.rel
        if href then
            if rel == THUMBNAIL or rel == "http://opds-spec.org/thumbnail" then
                entry.thumbnail = href
            elseif rel == IMAGE or rel == "http://opds-spec.org/cover" then
                entry.image = href
            elseif isAcquisition(rel) then
                entry.acquisitions[#entry.acquisitions + 1] = {
                    href = href,
                    type = link.type,
                    title = link.title,
                    length = tonumber(link.length),
                }
            elseif rel == "subsection" or rel == "http://opds-spec.org/sort/new"
                    or rel == "http://opds-spec.org/sort/popular" or rel == "related"
                    or ((rel == nil or rel == "alternate") and link.type and link.type:find("opds%-catalog")) then
                entry.navigation = entry.navigation or href
            end
        end
    end
    entry.thumbnail = entry.thumbnail or entry.image
    entry.image = entry.image or entry.thumbnail
    entry.is_book = #entry.acquisitions > 0
    return entry
end

--- Parses a feed. Returns a table with `title`, `entries` and the feed's links
-- (`next`, `previous`, `up`, `start`, `search` as a URL template with
-- {searchTerms}), or nil and an error.
function OPDS.parse(xml, base)
    if type(xml) ~= "string" or not xml:find("<feed", 1, true) then
        return nil, "not an OPDS feed"
    end
    -- Comments and the XML declaration never matter here
    xml = xml:gsub("<!%-%-.-%-%->", "")
    local first_entry = xml:find("<entry%f[%s>]") or #xml
    local head = xml:sub(1, first_entry - 1)
    local feed = {
        title = element(head, "title") or "",
        entries = {},
    }
    for _, link in ipairs(links(head)) do
        local href = OPDS.absolute(base, link.href)
        local rel = link.rel
        if href and rel then
            if rel == "next" or rel == "previous" or rel == "up" or rel == "start" then
                feed[rel] = href
            elseif rel == "search" and href:find("{searchTerms}", 1, true) then
                feed.search = href
            elseif rel == "search" then
                feed.search_description = href
            end
        end
    end
    for body in xml:gmatch("<entry%f[%s>][^>]*>(.-)</entry>") do
        feed.entries[#feed.entries + 1] = parseEntry(body, base)
    end
    return feed
end

--- Reads the search template out of an OpenSearch description document.
function OPDS.searchTemplate(xml, base)
    if type(xml) ~= "string" then return nil end
    local fallback
    for tag in xml:gmatch("<Url%f[%s/>][^>]*>") do
        local attrs = attributes(tag)
        if attrs.template then
            local href = OPDS.absolute(base, attrs.template)
            if attrs.type and attrs.type:find("atom") then return href end
            fallback = fallback or href
        end
    end
    return fallback
end

--- The best download of a book: EPUB first, then the formats KOReader reads well.
function OPDS.bestAcquisition(entry)
    local order = {
        ["application/epub+zip"] = 1,
        ["application/x-cbz"] = 2,
        ["application/vnd.comicbook+zip"] = 2,
        ["application/pdf"] = 3,
        ["application/x-fictionbook+xml"] = 4,
        ["application/x-mobipocket-ebook"] = 5,
    }
    local best, rank
    for _, acquisition in ipairs(entry.acquisitions or {}) do
        local r = order[acquisition.type or ""] or 10
        if not best or r < rank then
            best, rank = acquisition, r
        end
    end
    return best
end

--- A safe file name for a download, from its URL.
function OPDS.fileName(entry, acquisition)
    local href = acquisition.href:gsub("[?#].*$", "")
    local name = href:match("([^/]+)$") or ""
    name = name:gsub("%%(%x%x)", function(hex) return string.char(tonumber(hex, 16)) end)
    if not name:match("%.%w+$") then
        local ext = ({
            ["application/epub+zip"] = "epub",
            ["application/pdf"] = "pdf",
            ["application/x-cbz"] = "cbz",
            ["application/vnd.comicbook+zip"] = "cbz",
            ["application/x-fictionbook+xml"] = "fb2",
            ["application/x-mobipocket-ebook"] = "mobi",
        })[acquisition.type or ""] or "epub"
        name = (entry.title ~= "" and entry.title or "book") .. "." .. ext
    end
    name = name:gsub("[/\\:*?\"<>|%c]", "_")
    return name
end

return OPDS

-- Tests the plugin's OPDS reader, live-event parsing and conflict rules, which need no KOReader.
-- Run from the repository root: luajit koreader/test/library_test.lua koreader/opdshelf.koplugin
package.path = arg[1] .. "/?.lua;" .. package.path
local OPDS = require("opdshelf_opds")
local Stream = require("opdshelf_stream")
local Sync = require("opdshelf_sync")

local function check(cond, msg) if not cond then error("FAILED: " .. msg, 2) end print("ok - " .. msg) end

-- OPDS feeds, as OPDShelf writes them
local feed = OPDS.parse([[<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/" xmlns:calibre="http://calibre.kovidgoyal.net/2009/metadata">
<id>urn:opdshelf:books</id><title>Recently added</title>
<link rel="next" href="https://books.example/opds/books?page=2" type="application/atom+xml;profile=opds-catalog;kind=acquisition"/>
<link rel="search" href="/opds/search?q={searchTerms}" type="application/atom+xml"/>
<!-- <entry><title>commented out</title></entry> -->
<entry><title>El hobbit &amp; otros</title><id>urn:opdshelf:book:El%20hobbit.epub</id>
<author><name>J. R. R. Tolkien</name></author><author><name>Otro</name></author>
<dc:language>es</dc:language><calibre:series>La Tierra Media</calibre:series><calibre:series_index>1</calibre:series_index>
<category term="Fantasía" label="Fantasía"/>
<summary type="text">Bilbo &lt;3 &#233;</summary>
<link rel="http://opds-spec.org/image" href="https://books.example/cover/El%20hobbit.epub?v=1" type="image/jpeg"/>
<link rel="http://opds-spec.org/image/thumbnail" href="https://books.example/thumb/El%20hobbit.epub?v=1" type="image/jpeg"/>
<link rel="http://opds-spec.org/acquisition" href="https://books.example/download/Tolkien/El%20hobbit.epub" type="application/epub+zip" length="123456" title="EPUB"/>
</entry>
<entry><title>Authors</title><id>urn:opdshelf:authors</id><content type="text">12 authors</content>
<link rel="subsection" href="/opds/authors" type="application/atom+xml;profile=opds-catalog;kind=navigation"/></entry>
</feed>]], "https://books.example/opds/books")
check(feed.title == "Recently added" and #feed.entries == 2, "reads the feed and its entries, not comments")
check(feed.next == "https://books.example/opds/books?page=2", "finds the next page")
check(feed.search == "https://books.example/opds/search?q={searchTerms}", "and the search, made absolute")
local book = feed.entries[1]
check(book.title == "El hobbit & otros" and book.authors[2] == "Otro", "title and authors")
check(book.series == "La Tierra Media" and book.series_index == "1" and book.language == "es", "series and language")
check(book.summary == "Bilbo <3 é" and book.categories[1] == "Fantasía", "summary with entities and subjects")
check(book.thumbnail:match("/thumb/") and book.image:match("/cover/"), "cover and thumbnail")
check(book.is_book and OPDS.bestAcquisition(book).length == 123456, "its download")
check(OPDS.fileName(book, OPDS.bestAcquisition(book)) == "El hobbit.epub", "a file name from the download URL")
local nav = feed.entries[2]
check(not nav.is_book and nav.navigation == "https://books.example/opds/authors" and nav.summary == "12 authors", "navigation entries")
check(OPDS.parse("<html></html>", "x") == nil, "rejects what is not a feed")
check(OPDS.absolute("https://a.b/c/d", "e") == "https://a.b/c/e" and OPDS.absolute("https://a.b:8/c", "?p=2") == "https://a.b:8/c?p=2", "relative links")
check(OPDS.bestAcquisition({ acquisitions = { { type = "application/pdf" }, { type = "application/epub+zip" } } }).type == "application/epub+zip", "prefers EPUB")
check(OPDS.fileName({ title = "A/B" }, { href = "https://x/download?id=3", type = "application/pdf" }) == "A_B.pdf", "names downloads without a file name")

-- Server-sent events arriving in pieces
local events = {}
local parse = Stream.parser(function(name, data) events[#events + 1] = { name, data } end)
parse("event: hello\ndata: {\"server_time\":1}\n\n: keep-alive\n\nevent: progress\nda")
parse("ta: {\"a\":1}\r\n\r\ndata: x\ndata: y\n\n")
check(#events == 3 and events[1][1] == "hello" and events[2][2] == '{"a":1}', "splits events, even cut in the middle")
check(events[3][1] == "message" and events[3][2] == "x\ny", "joins multi-line data and skips comments")
local dechunk = Stream.dechunker()
local out = dechunk("5\r\nhel") .. dechunk("lo\r\n3\r") .. dechunk("\nabc\r\n0\r\n\r\n")
check(out == "helloabc", "decodes chunked bodies fed in pieces")

-- Conflicts
local now = 1000
local function r(device, ts, extra)
    local t = { progress = "/x" .. device, percentage = 0.5, device = device, device_id = device, timestamp = ts }
    for k, v in pairs(extra or {}) do t[k] = v end
    return t
end
local base = { me = "kobo", reason = "open", idle = 1000 }
local function decide(o) for k, v in pairs(base) do if o[k] == nil then o[k] = v end end return Sync.decide(o) end
check(decide{ remote = nil } == "none" and decide{ remote = nil, moved = true } == "push", "no position on the server: send ours if we read")
check(decide{ remote = r("kobo", now) } == "none", "our own position")
check(decide{ remote = r("phone", now), showing = true } == "none", "already on that page")
check(decide{ remote = r("phone", now), ahead = true } == "jump", "newer and further: go there")
check(decide{ remote = r("phone", now), ahead = false } == "ask", "newer but behind: ask")
check(decide{ remote = r("phone", now), ahead = true, forward = "ask" } == "ask", "settings change it")
check(decide{ remote = r("phone", now), ahead = true, forward = "ignore" } == "none", "or turn it off")
check(decide{ remote = r("phone", now), synced = r("phone", now) } == "none", "a position already seen is not offered again")
check(decide{ remote = r("phone", now), synced = r("phone", now), moved = true } == "push", "and then our reading wins")
check(decide{ remote = r("phone", now), synced = r("kobo", now - 50), moved = true } == "conflict", "both read since they agreed: conflict")
check(decide{ remote = r("phone", now), synced = r("kobo", now - 50), moved = true, reason = "rejected" } == "conflict", "also when the server kept the other one")
check(decide{ remote = r("phone", now), reason = "live", idle = 5, ahead = true } == "ask", "while reading, ask instead of moving the page")
check(decide{ remote = r("phone", now), reason = "live", idle = 5, moved = true, ahead = true } == "ask", "even with unsent reading")
check(decide{ remote = r("web", now, { manual = true }), ahead = true } == "ask", "points set by hand are always offered")
check(decide{ remote = r("phone", now), synced = r("phone", now), reason = "manual" } == "ask", "syncing by hand offers it again")
check(Sync.ago(now - 30, now, function(s) return s end, function(s, n) return (s:gsub("%%1", n)) end) == "just now"
    and Sync.ago(now - 7200, now, function(s) return s end, function(s, n) return (s:gsub("%%1", n)) end) == "2 h ago", "says how long ago")
print("all good")

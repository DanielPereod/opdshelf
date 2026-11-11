--[[--
A server-sent events client that never blocks KOReader.

The socket is non-blocking and the caller calls `step()` from time to time
(from UIManager:scheduleIn), which reads whatever arrived and hands each
complete event to `on_event(name, data)`.

@module opdshelf_stream
--]]

local Stream = {}
Stream.__index = Stream

--- Decodes HTTP chunked transfer encoding fed in pieces.
function Stream.dechunker()
    local buffer, remaining, done = "", nil, false
    return function(data)
        if done then return "" end
        buffer = buffer .. data
        local out = {}
        while true do
            if remaining == nil then
                local line_end = buffer:find("\r\n", 1, true)
                if not line_end then break end
                local size = tonumber(buffer:sub(1, line_end - 1):match("^%x+") or "", 16)
                buffer = buffer:sub(line_end + 2)
                if not size then done = true break end
                if size == 0 then done = true break end
                remaining = size
            else
                if #buffer < remaining + 2 then
                    if #buffer > remaining then break end
                    out[#out + 1] = buffer
                    remaining = remaining - #buffer
                    buffer = ""
                    break
                end
                out[#out + 1] = buffer:sub(1, remaining)
                buffer = buffer:sub(remaining + 3)
                remaining = nil
            end
        end
        return table.concat(out)
    end
end

--- Splits a text/event-stream fed in pieces into events.
function Stream.parser(on_event)
    local buffer = ""
    local name, data = nil, {}
    local function dispatch()
        if #data > 0 then
            on_event(name or "message", table.concat(data, "\n"))
        end
        name, data = nil, {}
    end
    return function(text)
        buffer = buffer .. text:gsub("\r\n", "\n"):gsub("\r", "\n")
        while true do
            local line_end = buffer:find("\n", 1, true)
            if not line_end then break end
            local line = buffer:sub(1, line_end - 1)
            buffer = buffer:sub(line_end + 1)
            if line == "" then
                dispatch()
            elseif line:sub(1, 1) ~= ":" then
                local field, value = line:match("^([^:]*):? ?(.*)$")
                if field == "event" then
                    name = value
                elseif field == "data" then
                    data[#data + 1] = value
                end
            end
        end
    end
end

--[[--
@param opts table
  url: http(s) URL of the stream
  headers: extra request headers
  on_event(name, data): called for each event
  on_open(): called once the server accepted the stream
  socket, ssl: LuaSocket and LuaSec, for tests
--]]
function Stream.new(opts)
    local self = setmetatable(opts, Stream)
    self.socket = self.socket or require("socket")
    return self
end

local function wouldBlock(err)
    return err == "timeout" or err == "wantread" or err == "wantwrite" or err == "Operation already in progress"
end

function Stream:run()
    local scheme, host, port, path = self.url:match("^(https?)://([^/:]+):?(%d*)(.*)$")
    if not scheme then return nil, "invalid url" end
    port = tonumber(port) or (scheme == "https" and 443 or 80)
    if path == "" then path = "/" end

    local sock = self.socket.tcp()
    self.sock = sock
    sock:settimeout(0)
    while true do
        local ok, err = sock:connect(host, port)
        if ok or err == "already connected" then break end
        if not wouldBlock(err) then return nil, "connect: " .. tostring(err) end
        coroutine.yield()
    end
    if scheme == "https" then
        local ssl = self.ssl or require("ssl")
        local wrapped, err = ssl.wrap(sock, {
            mode = "client", protocol = "any", verify = "none",
            options = { "all", "no_sslv2", "no_sslv3" },
        })
        if not wrapped then return nil, "tls: " .. tostring(err) end
        sock = wrapped
        self.sock = sock
        sock:settimeout(0)
        if sock.sni then sock:sni(host) end
        while true do
            local ok, herr = sock:dohandshake()
            if ok then break end
            if not wouldBlock(herr) then return nil, "tls: " .. tostring(herr) end
            coroutine.yield()
        end
    end

    -- HTTP/1.0, so the server ends the body by closing instead of chunking it
    local lines = {
        "GET " .. path .. " HTTP/1.0",
        "Host: " .. host .. ((port == 80 or port == 443) and "" or (":" .. port)),
        "Accept: text/event-stream",
        "Cache-Control: no-cache",
    }
    for k, v in pairs(self.headers or {}) do
        lines[#lines + 1] = k .. ": " .. v
    end
    local request = table.concat(lines, "\r\n") .. "\r\n\r\n"
    local sent = 0
    while sent < #request do
        local last, err, partial = sock:send(request, sent + 1)
        if last then
            sent = last
        elseif wouldBlock(err) then
            sent = partial or sent
            coroutine.yield()
        else
            return nil, "send: " .. tostring(err)
        end
    end

    local head = ""
    local body_feed
    local parse = Stream.parser(function(name, data)
        if self.on_event then self.on_event(name, data) end
    end)
    while true do
        local data, err, partial = sock:receive(8192)
        local piece = data or partial
        if piece and #piece > 0 then
            if body_feed then
                parse(body_feed(piece))
            else
                head = head .. piece
                local head_end = head:find("\r\n\r\n", 1, true)
                if head_end then
                    local status = tonumber(head:match("^HTTP/%d%.%d (%d%d%d)"))
                    if status ~= 200 then
                        return nil, "http " .. tostring(status), status
                    end
                    local chunked = head:sub(1, head_end):lower():find("transfer%-encoding:%s*chunked")
                    body_feed = chunked and Stream.dechunker() or function(s) return s end
                    self.opened = true
                    if self.on_open then self.on_open() end
                    local rest = head:sub(head_end + 4)
                    head = nil
                    if #rest > 0 then parse(body_feed(rest)) end
                elseif #head > 16384 then
                    return nil, "bad response"
                end
            end
        end
        if err == "closed" then
            return nil, "closed"
        elseif err and not wouldBlock(err) then
            return nil, "receive: " .. tostring(err)
        end
        if not data then coroutine.yield() end
    end
end

--- Reads what arrived. Returns true while the stream is alive, or false and why it ended.
function Stream:step()
    if self.ended then return false, self.error end
    if not self.co then
        self.co = coroutine.create(function() return self:run() end)
    end
    local ok, result, err, status = coroutine.resume(self.co)
    if not ok then
        self:close()
        self.error = tostring(result)
        return false, self.error
    end
    if coroutine.status(self.co) == "dead" then
        self:close()
        self.error, self.status = err or "ended", status
        return false, self.error, status
    end
    return true
end

function Stream:close()
    self.ended = true
    if self.sock then
        pcall(self.sock.close, self.sock)
        self.sock = nil
    end
end

return Stream

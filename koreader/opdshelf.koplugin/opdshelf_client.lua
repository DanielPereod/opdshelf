--[[--
HTTP calls to an OPDShelf server: its progress API, OPDS feeds, covers and downloads.

The account is the server's own login (the one used for its OPDS catalog),
sent as HTTP Basic credentials.

@module opdshelf_client
--]]

local Client = {}
Client.__index = Client

-- Seconds to wait for a connection, and for the whole request
Client.TIMEOUT = { 5, 10 }
Client.FILE_TIMEOUT = { 15, 600 }

--[[--
@param o table
  server: base URL, like https://books.example
  username, password
  http, ltn12, json, mime, socketutil: libraries, injected by tests
--]]
function Client.new(o)
    local self = setmetatable(o or {}, Client)
    self.http = self.http or require("socket.http")
    self.ltn12 = self.ltn12 or require("ltn12")
    self.json = self.json or require("json")
    self.mime = self.mime or require("mime")
    local ok, socketutil = pcall(require, "socketutil")
    self.socketutil = self.socketutil or (ok and socketutil or nil)
    self.server = (self.server or ""):gsub("/+$", "")
    -- Pasting the old Progress sync address works too
    self.server = self.server:gsub("/kosync$", "")
    return self
end

function Client:configured()
    return self.server ~= "" and self.server:match("^https?://") ~= nil
end

function Client:url(path)
    if path:match("^https?://") then return path end
    return self.server .. path
end

function Client:authorization()
    if not self.username or self.username == "" then return nil end
    return "Basic " .. self.mime.b64(self.username .. ":" .. (self.password or ""))
end

--- Only send the password to our own server, not to wherever a feed links.
function Client:sameOrigin(url)
    local origin = self.server:match("^(https?://[^/]+)")
    return origin and url:sub(1, #origin) == origin and (#url == #origin or url:sub(#origin + 1, #origin + 1):match("[/?#]"))
end

function Client:headers(url, extra)
    local headers = { ["User-Agent"] = "OPDShelf-KOReader" }
    if self:sameOrigin(url) then
        headers["Authorization"] = self:authorization()
    end
    for k, v in pairs(extra or {}) do headers[k] = v end
    return headers
end

local function setTimeout(self, t)
    if self.socketutil then self.socketutil:set_timeout(t[1], t[2]) end
end

local function resetTimeout(self)
    if self.socketutil then self.socketutil:reset_timeout() end
end

--[[--
Makes a request and returns the status code (a number), the body and the
response headers. On a network failure the code is nil and the body says why.
--]]
function Client:raw(method, path, body, extra_headers, sink, timeout)
    local url = self:url(path)
    local chunks = {}
    local headers = self:headers(url, extra_headers)
    local source
    if body then
        headers["Content-Length"] = tostring(#body)
        source = self.ltn12.source.string(body)
    end
    setTimeout(self, timeout or Client.TIMEOUT)
    local ok, code, response_headers, status = pcall(function()
        local _, c, h, s = self.http.request{
            url = url,
            method = method,
            headers = headers,
            source = source,
            sink = sink or self.ltn12.sink.table(chunks),
        }
        return c, h, s
    end)
    resetTimeout(self)
    if not ok then
        return nil, tostring(code)
    end
    if type(code) ~= "number" then
        return nil, tostring(status or code)
    end
    return code, table.concat(chunks), response_headers or {}
end

--- A JSON request. Returns the code and the decoded body (nil if it is not JSON).
function Client:json_request(method, path, value)
    local body = value ~= nil and self.json.encode(value) or nil
    local code, text, headers = self:raw(method, path, body, {
        ["Accept"] = "application/json",
        ["Content-Type"] = body and "application/json" or nil,
    })
    if not code then return nil, text end
    local ok, decoded = pcall(self.json.decode, text)
    return code, ok and decoded or nil, headers
end

function Client:getProgress(document)
    return self:json_request("GET", "/api/progress/" .. document)
end

function Client:putProgress(document, record)
    return self:json_request("PUT", "/api/progress/" .. document, record)
end

function Client:getAnnotations(document)
    return self:json_request("GET", "/api/annotations/" .. document)
end

function Client:putAnnotations(document, batch)
    return self:json_request("PUT", "/api/annotations/" .. document, batch)
end

--- Fetches a text resource (a feed). Returns code, body, headers.
function Client:fetch(url)
    return self:raw("GET", url, nil, { ["Accept"] = "application/atom+xml, application/xml;q=0.9, */*;q=0.5" })
end

--- Downloads `url` to `path` through a temporary file. Returns true, or false and why.
function Client:download(url, path, timeout)
    local tmp = path .. ".part"
    local file = io.open(tmp, "wb")
    if not file then return false, "cannot write " .. tmp end
    local code, err = self:raw("GET", url, nil, { ["Accept-Encoding"] = "identity" },
        self.ltn12.sink.file(file), timeout or Client.FILE_TIMEOUT)
    -- ltn12.sink.file closes the file at the end; make sure on failures too
    pcall(file.close, file)
    if code == 200 then
        os.remove(path)
        local ok, rename_err = os.rename(tmp, path)
        if not ok then
            os.remove(tmp)
            return false, rename_err
        end
        return true
    end
    os.remove(tmp)
    return false, code and ("HTTP " .. code) or err, code
end

return Client

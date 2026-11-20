package io.github.danielpereod.opdshelf.data

import java.net.URI
import java.net.URLDecoder

/**
 * The server and login in the QR code of the web panel (Sync › Connect your apps):
 * `opdshelf://connect?server=…&username=…&password=…`
 */
data class ConnectLink(val server: String, val username: String, val password: String) {
    companion object {
        fun parse(text: String?): ConnectLink? {
            val uri = runCatching { URI(text?.trim().orEmpty()) }.getOrNull() ?: return null
            if (!uri.scheme.equals("opdshelf", ignoreCase = true) || !uri.host.equals("connect", ignoreCase = true)) return null
            val params = uri.rawQuery.orEmpty().split('&').filter { it.isNotEmpty() }.associate { pair ->
                val parts = pair.split('=', limit = 2)
                decode(parts[0]) to decode(parts.getOrElse(1) { "" })
            }
            val server = params["server"]?.takeIf { it.isNotBlank() } ?: return null
            return ConnectLink(server, params["username"].orEmpty(), params["password"].orEmpty())
        }

        private fun decode(s: String) = runCatching { URLDecoder.decode(s, "UTF-8") }.getOrDefault(s)
    }
}

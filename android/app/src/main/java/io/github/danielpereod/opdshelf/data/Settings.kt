package io.github.danielpereod.opdshelf.data

import android.content.Context
import android.os.Build
import java.util.UUID

/** Server address and login, plus the id this device syncs positions under. */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)

    var serverUrl: String
        get() = prefs.getString("server", "") ?: ""
        set(value) = prefs.edit().putString("server", normalizeServerUrl(value)).apply()

    var username: String
        get() = prefs.getString("username", "") ?: ""
        set(value) = prefs.edit().putString("username", value.trim()).apply()

    var password: String
        get() = prefs.getString("password", "") ?: ""
        set(value) = prefs.edit().putString("password", value).apply()

    val isConfigured: Boolean get() = serverUrl.isNotEmpty()

    val deviceId: String
        get() = prefs.getString("device_id", null) ?: UUID.randomUUID().toString().replace("-", "").also {
            prefs.edit().putString("device_id", it).apply()
        }

    var deviceName: String
        get() = prefs.getString("device_name", null) ?: defaultDeviceName()
        set(value) = prefs.edit().putString("device_name", value.trim().ifEmpty { defaultDeviceName() }).apply()

    private fun defaultDeviceName(): String {
        val model = Build.MODEL ?: "Android"
        val maker = Build.MANUFACTURER ?: ""
        return if (model.startsWith(maker, ignoreCase = true)) model else "$maker $model".trim()
    }
}

/** "books.example.com/" → "https://books.example.com" */
fun normalizeServerUrl(value: String): String {
    var url = value.trim().trimEnd('/')
    if (url.isEmpty()) return ""
    if (!url.startsWith("http://") && !url.startsWith("https://")) {
        // Servers at home are usually plain http; anything else is expected to have https
        url = if (isLocalHost(url.substringBefore('/').substringBeforeLast(':'))) "http://$url" else "https://$url"
    }
    return url
}

private fun isLocalHost(host: String): Boolean {
    val h = host.lowercase()
    if (h == "localhost" || h.endsWith(".local") || h.endsWith(".lan") || h.endsWith(".home.arpa")) return true
    val parts = h.split('.').map { it.toIntOrNull() ?: return false }
    if (parts.size != 4) return false
    return parts[0] == 10 || parts[0] == 127 || (parts[0] == 192 && parts[1] == 168) ||
        (parts[0] == 172 && parts[1] in 16..31) || (parts[0] == 100 && parts[1] in 64..127)
}

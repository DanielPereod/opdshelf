package io.github.danielpereod.opdshelf.data

import android.content.Context
import android.os.Build
import java.util.UUID

/** Server address and login, plus the id this device syncs positions under. */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE)
    private val resolver = context.applicationContext.contentResolver

    var serverUrl: String
        get() = prefs.getString("server", "") ?: ""
        set(value) = prefs.edit().putString("server", normalizeServerUrl(value)).apply()

    var username: String
        get() = prefs.getString("username", "") ?: ""
        set(value) = prefs.edit().putString("username", value.trim()).apply()

    var password: String
        get() = prefs.getString("password", "") ?: ""
        set(value) = prefs.edit().putString("password", value).apply()

    /** When positions were last all sent, in milliseconds, 0 if never. */
    var lastSync: Long
        get() = prefs.getLong("last_sync", 0)
        set(value) = prefs.edit().putLong("last_sync", value).apply()

    /** Whether the reader already explained how to set an exact last position. */
    var markHintShown: Boolean
        get() = prefs.getBoolean("mark_hint_shown", false)
        set(value) = prefs.edit().putBoolean("mark_hint_shown", value).apply()

    /** The app's look: 0 follows the system, 1 light, 2 dark. */
    var appearance: Int
        get() = prefs.getInt("appearance", 0)
        set(value) = prefs.edit().putInt("appearance", value).apply()

    /** Whether Explore shows books as a grid of covers (or as a list); kept across restarts and theme changes. */
    var exploreGrid: Boolean
        get() = prefs.getBoolean("explore_grid", true)
        set(value) = prefs.edit().putBoolean("explore_grid", value).apply()

    fun forgetServer() {
        prefs.edit().remove("server").remove("username").remove("password").apply()
    }

    val isConfigured: Boolean get() = serverUrl.isNotEmpty()

    val deviceId: String
        get() = prefs.getString("device_id", null) ?: UUID.randomUUID().toString().replace("-", "").also {
            prefs.edit().putString("device_id", it).apply()
        }

    var deviceName: String
        get() = prefs.getString("device_name", null) ?: defaultDeviceName()
        set(value) = prefs.edit().putString("device_name", value.trim().ifEmpty { defaultDeviceName() }).apply()

    /** The name the owner gave this device in the system's settings, else its maker and model. Changes nothing of the device id. */
    private fun defaultDeviceName(): String {
        // Both are readable without any permission; some makers keep only one of them
        val named = runCatching {
            android.provider.Settings.Global.getString(resolver, android.provider.Settings.Global.DEVICE_NAME)
        }.getOrNull()
        val bluetooth = runCatching { android.provider.Settings.Secure.getString(resolver, "bluetooth_name") }.getOrNull()
        return friendlyDeviceName(named, bluetooth, Build.MANUFACTURER, Build.MODEL)
    }
}

/**
 * A device's name as people know it: the one given in the system's settings
 * ("Xiaomi Pad 6", "Dani's tablet"), or the Bluetooth one, unless they are
 * just the model's code; else maker and model ("Xiaomi 23043RP34G").
 */
fun friendlyDeviceName(deviceName: String?, bluetoothName: String?, manufacturer: String?, model: String?): String {
    val code = model?.trim().orEmpty()
    fun usable(name: String?) = name?.trim()?.takeIf { it.isNotEmpty() && !it.equals(code, ignoreCase = true) }
    usable(deviceName)?.let { return it }
    usable(bluetoothName)?.let { return it }
    val maker = manufacturer?.trim().orEmpty().replaceFirstChar { it.uppercase() }
    val shown = code.ifEmpty { "Android" }
    return if (shown.startsWith(maker, ignoreCase = true)) shown else "$maker $shown".trim()
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

/** What is wrong with a server address, as normalizeServerUrl leaves it. */
enum class AddressProblem { Invalid, Port }

/** Null when [url] (normalized) can be connected to; otherwise why not, to say it in words. */
fun addressProblem(url: String): AddressProblem? {
    val match = Regex("""^https?://([^/:?#\[\]\s]+|\[[0-9A-Fa-f:.]+])(:[^/?#]*)?([/?#]\S*)?$""").matchEntire(url)
        ?: return AddressProblem.Invalid
    val host = match.groupValues[1]
    if (!host.startsWith("[")) {
        val labels = host.split('.')
        if (labels.any { it.isEmpty() || !it.all { c -> c.isLetterOrDigit() || c == '-' } }) return AddressProblem.Invalid
    }
    val port = match.groups[2]?.value?.removePrefix(":")
    if (port != null && (port.toIntOrNull() ?: 0) !in 1..65535) return AddressProblem.Port
    return null
}

private fun isLocalHost(host: String): Boolean {
    val h = host.lowercase()
    if (h == "localhost" || h.endsWith(".local") || h.endsWith(".lan") || h.endsWith(".home.arpa")) return true
    val parts = h.split('.').map { it.toIntOrNull() ?: return false }
    if (parts.size != 4) return false
    return parts[0] == 10 || parts[0] == 127 || (parts[0] == 192 && parts[1] == 168) ||
        (parts[0] == 172 && parts[1] in 16..31) || (parts[0] == 100 && parts[1] in 64..127)
}

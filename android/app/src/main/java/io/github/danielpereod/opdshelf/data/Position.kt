package io.github.danielpereod.opdshelf.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Text around a position, to find it again when its pointer does not match. */
@Serializable
data class PositionContext(
    val before: String = "",
    val after: String = "",
    val selected: String = "",
)

/**
 * A reading position as OPDShelf stores it. `progress` is a KOReader xpointer
 * for EPUB and the page number (from 1) for PDFs and comics.
 */
@Serializable
data class Position(
    val progress: String = "",
    val percentage: Double = 0.0,
    val device: String = "",
    @SerialName("device_id") val deviceId: String = "",
    val cfi: String? = null,
    val context: PositionContext? = null,
    val manual: Boolean = false,
    /** When it was read, in Unix seconds. */
    val timestamp: Long = 0,
)

@Serializable
data class ProgressResponse(
    val document: String = "",
    val current: Position? = null,
)

/** This device's last position for a book, and whether the server has it yet. */
@Serializable
data class LocalPosition(
    val position: Position,
    val pending: Boolean,
    /** Readium's own locator for this position, to go back to it exactly on this device. */
    val locator: String? = null,
)

/** Where to open a book, and the other device it comes from, if any. */
data class StartPosition(val position: Position?, val locator: String?, val fromDevice: String?)

/**
 * Picks where to open a book from this device's last position and the server's.
 * A position read offline that the server does not have yet wins when it is newer.
 */
fun chooseStart(local: LocalPosition?, server: Position?, deviceId: String): StartPosition {
    if (local != null && local.pending && (server == null || local.position.timestamp >= server.timestamp)) {
        return StartPosition(local.position, local.locator, null)
    }
    if (server == null) return StartPosition(local?.position, local?.locator, null)
    val sameAsLocal = local != null && local.position.progress == server.progress
    val from = if (server.deviceId != deviceId && !sameAsLocal) server.device else null
    return StartPosition(server, if (sameAsLocal) local?.locator else null, from)
}

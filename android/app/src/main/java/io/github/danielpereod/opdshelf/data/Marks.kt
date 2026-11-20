package io.github.danielpereod.opdshelf.data

import kotlinx.serialization.Serializable

/**
 * A book marked as finished ("read") or not finished ("unread"), as the server
 * keeps it for the web panel, KOReader and this app. [at] is in Unix seconds:
 * the newest mark of a book wins, whichever device made it.
 */
@Serializable
data class ReadMark(val state: String, val at: Long) {
    val finished: Boolean get() = state == READ

    companion object {
        const val READ = "read"
        const val UNREAD = "unread"

        fun of(finished: Boolean, at: Long) = ReadMark(if (finished) READ else UNREAD, at)
    }
}

/**
 * The book with the server's mark applied, if it is newer than the one this
 * device knows. A mark made here that the server does not have yet ([pending])
 * is kept: it is sent next, and the server decides which one is newer.
 */
fun LocalBook.withMark(server: ReadMark?, pending: Boolean): LocalBook {
    if (pending || server == null || server.at <= markedAt) return this
    return copy(finishedByHand = server.finished, markedAt = server.at)
}

/**
 * The marks to queue for books marked by hand before marks were synced: those with
 * [LocalBook.finishedByHand] set, no [LocalBook.markedAt] and nothing queued. Their time is
 * when an old build says it was marked, or [fallback] (when the list of books was last
 * saved), never "now": that would hide readings made on other devices since.
 */
fun marksToMigrate(books: List<LocalBook>, pending: Map<String, ReadMark>, fallback: Long): Map<String, ReadMark> =
    books
        .filter { it.finishedByHand != null && it.markedAt == 0L && it.document !in pending }
        .associate { it.document to ReadMark.of(it.finishedByHand == true, it.legacyFinishedAt.takeIf { at -> at > 0 } ?: fallback) }

/** Every book's position and mark on the server (`GET /api/progress`). */
@Serializable
data class LibraryProgress(
    val books: List<BookProgress> = emptyList(),
    val marks: List<MarkEntry> = emptyList(),
)

@Serializable
data class BookProgress(
    val document: String = "",
    /** Null when the file is no longer in the library. */
    val path: String? = null,
    val current: Position? = null,
)

@Serializable
data class MarkEntry(
    val document: String = "",
    val path: String? = null,
    val state: String = "",
    val at: Long = 0,
)

/** How far a book of the library was read, as every device sees it. */
data class RemoteReading(val percentage: Double, val finished: Boolean)

fun LibraryProgress.marksByDocument(): Map<String, ReadMark> =
    marks.filter { it.document.isNotEmpty() }.associate { it.document to ReadMark(it.state, it.at) }

/**
 * Where each started or finished book of the library is at, by its path, worked
 * out as the server does for the web panel: "read" marks a book finished,
 * "unread" hides the positions saved before it, otherwise it goes by how far
 * it was read.
 */
fun LibraryProgress.byPath(): Map<String, RemoteReading> {
    val marks = marksByDocument()
    val out = mutableMapOf<String, RemoteReading>()
    for (book in books) {
        val path = book.path ?: continue
        val current = book.current ?: continue
        val mark = marks[book.document]
        val reading = when {
            mark?.finished == true -> RemoteReading(1.0, true)
            mark != null && current.timestamp <= mark.at -> continue
            current.percentage >= LocalBook.FINISHED_AT -> RemoteReading(current.percentage, true)
            current.percentage > 0 -> RemoteReading(current.percentage, false)
            else -> continue
        }
        out[path] = reading
    }
    for (mark in this.marks) {
        val path = mark.path ?: continue
        if (mark.state == ReadMark.READ && path !in out) out[path] = RemoteReading(1.0, true)
    }
    return out
}

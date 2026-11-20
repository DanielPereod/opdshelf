package io.github.danielpereod.opdshelf.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer
import java.io.File

/** A book downloaded to the device. */
@Serializable
data class LocalBook(
    /** The OPDS entry id. */
    val id: String,
    val title: String,
    val authors: List<String> = emptyList(),
    val fileName: String,
    val mediaType: String,
    /** KOReader's id of the file, which positions are synced under. */
    val document: String,
    val coverFile: String? = null,
    val addedAt: Long = System.currentTimeMillis(),
    /** From 0 to 1, as last read on any device. */
    val percentage: Double = 0.0,
    val summary: String? = null,
    /** As the catalog names it, e.g. EPUB. */
    val format: String? = null,
    val sizeBytes: Long = 0,
    /** When it was last opened on this device, 0 if never. */
    val lastOpenedAt: Long = 0,
    /** Marked as finished (true) or not (false) by hand; null goes by how far it was read. */
    val finishedByHand: Boolean? = null,
    /** When [finishedByHand] last changed, in Unix seconds (by the server's clock once it has it). */
    val finishedAt: Long = 0,
    /** [finishedByHand] changed on this device and the server does not have it yet. */
    val finishedPending: Boolean = false,
    val meta: BookMeta = BookMeta(),
) {
    /** Finished, as the web counts it: read to 98 %, unless marked otherwise. */
    val finished: Boolean get() = finishedByHand ?: (percentage >= FINISHED_AT)

    /**
     * The server's mark for a hand mark made here: "read" when finished.
     * Unmarking clears the server's mark (null), unless the book was read to
     * the end, which only "unread" hides.
     */
    val markToSend: String?
        get() = when {
            finishedByHand == true -> "read"
            percentage >= FINISHED_AT -> "unread"
            else -> null
        }

    /** Takes a mark made on another device, unless this one has a newer one not sent yet. */
    fun adopt(mark: Mark): LocalBook =
        if (finishedPending || mark.at <= finishedAt) this
        else copy(finishedByHand = mark.state == "read", finishedAt = mark.at)

    companion object {
        const val FINISHED_AT = 0.98
    }
}

/** A place kept in a book, to come back to. */
@Serializable
data class Bookmark(
    /** Readium's locator, as JSON. */
    val locator: String,
    val chapter: String? = null,
    val progression: Double = 0.0,
    /** A few words of the page, when known. */
    val text: String? = null,
    val createdAt: Long = System.currentTimeMillis(),
)

/** Downloaded books and reading positions, kept as JSON files in the app's storage. */
class LocalStore(private val dir: File) {
    val booksDir = File(dir, "books").apply { mkdirs() }
    private val booksFile = File(dir, "books.json")
    private val positionsFile = File(dir, "positions.json")
    private val bookmarksFile = File(dir, "bookmarks.json")
    private val bookmarksSerializer = MapSerializer(String.serializer(), ListSerializer(Bookmark.serializer()))
    private var bookmarks: Map<String, List<Bookmark>> = read(bookmarksFile, bookmarksSerializer) ?: emptyMap()

    private val positionsSerializer = MapSerializer(String.serializer(), LocalPosition.serializer())
    // Books marked by hand before marks were synced are sent once
    private val _books = MutableStateFlow(
        (read(booksFile, ListSerializer(LocalBook.serializer())) ?: emptyList()).map {
            if (it.finishedByHand != null && it.finishedAt == 0L) it.copy(finishedPending = true) else it
        }
    )
    val books: StateFlow<List<LocalBook>> = _books.asStateFlow()
    private var positions: Map<String, LocalPosition> = read(positionsFile, positionsSerializer) ?: emptyMap()

    private val annotationsFile = File(dir, "annotations.json")
    private val annotationsSerializer = MapSerializer(String.serializer(), LocalAnnotations.serializer())
    private val _annotations = MutableStateFlow(read(annotationsFile, annotationsSerializer) ?: emptyMap())
    /** Highlights of each book (by document id), as they change. */
    val annotations: StateFlow<Map<String, LocalAnnotations>> = _annotations.asStateFlow()

    fun book(id: String): LocalBook? = _books.value.firstOrNull { it.id == id }

    fun file(book: LocalBook): File = File(booksDir, book.fileName)

    @Synchronized
    fun addBook(book: LocalBook) {
        _books.value = _books.value.filterNot { it.id == book.id } + book
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
    }

    @Synchronized
    fun removeBook(book: LocalBook) {
        file(book).delete()
        book.coverFile?.let { File(booksDir, it).delete() }
        if (bookmarks.containsKey(book.id)) setBookmarks(book.id, emptyList())
        _books.value = _books.value.filterNot { it.id == book.id }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
    }

    @Synchronized
    fun setPercentage(document: String, percentage: Double) {
        _books.value = _books.value.map { if (it.document == document) it.copy(percentage = percentage) else it }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
    }

    @Synchronized
    fun markOpened(id: String) {
        _books.value = _books.value.map { if (it.id == id) it.copy(lastOpenedAt = System.currentTimeMillis()) else it }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
    }

    @Synchronized
    fun setFinished(id: String, finished: Boolean) {
        val now = System.currentTimeMillis() / 1000
        update { if (it.id == id) it.copy(finishedByHand = finished, finishedAt = now, finishedPending = true) else it }
    }

    /** Books marked (or unmarked) as finished here that the server does not have yet. */
    @Synchronized
    fun pendingMarks(): List<LocalBook> = _books.value.filter { it.finishedPending }

    /** Notes that the server took a mark, unless it changed again meanwhile; `at` is the server's time. */
    @Synchronized
    fun markFinishedSent(sent: LocalBook, at: Long) {
        update {
            if (it.id == sent.id && it.finishedPending && it.finishedByHand == sent.finishedByHand && it.finishedAt == sent.finishedAt) {
                it.copy(finishedPending = false, finishedAt = at.takeIf { t -> t > 0 } ?: sent.finishedAt)
            } else it
        }
    }

    /** Applies the server's marks (by document id) to the books on the device. */
    @Synchronized
    fun adoptMarks(marks: Map<String, Mark>) {
        if (_books.value.none { it.document in marks }) return
        update { book -> marks[book.document]?.let(book::adopt) ?: book }
    }

    private fun update(change: (LocalBook) -> LocalBook) {
        val next = _books.value.map(change)
        if (next == _books.value) return
        _books.value = next
        write(booksFile, ListSerializer(LocalBook.serializer()), next)
    }

    @Synchronized
    fun bookmarks(id: String): List<Bookmark> = bookmarks[id].orEmpty()

    @Synchronized
    fun setBookmarks(id: String, list: List<Bookmark>) {
        bookmarks = if (list.isEmpty()) bookmarks - id else bookmarks + (id to list)
        write(bookmarksFile, bookmarksSerializer, bookmarks)
    }

    /** Space the downloaded books take, in bytes. */
    fun usedBytes(): Long = _books.value.sumOf { file(it).length() }

    @Synchronized
    fun position(document: String): LocalPosition? = positions[document]

    @Synchronized
    fun savePosition(document: String, position: LocalPosition) {
        positions = positions + (document to position)
        write(positionsFile, positionsSerializer, positions)
    }

    /** Marks a position as sent, unless a newer one was saved meanwhile. */
    @Synchronized
    fun markSent(document: String, sent: Position) {
        val current = positions[document] ?: return
        if (current.position == sent) savePosition(document, current.copy(pending = false))
    }

    @Synchronized
    fun pending(): Map<String, Position> =
        positions.filterValues { it.pending }.mapValues { it.value.position }

    @Synchronized
    fun annotations(document: String): LocalAnnotations = _annotations.value[document] ?: LocalAnnotations()

    @Synchronized
    fun updateAnnotations(document: String, change: (LocalAnnotations) -> LocalAnnotations) {
        val next = change(annotations(document))
        _annotations.value = _annotations.value + (document to next)
        write(annotationsFile, annotationsSerializer, _annotations.value)
    }

    /** Highlight changes not sent yet, by book. */
    @Synchronized
    fun pendingAnnotations(): Map<String, List<Annotation>> =
        _annotations.value.mapValues { it.value.pending.values.toList() }.filterValues { it.isNotEmpty() }

    private fun <T> read(file: File, serializer: kotlinx.serialization.KSerializer<T>): T? =
        runCatching { json.decodeFromString(serializer, file.readText()) }.getOrNull()

    private fun <T> write(file: File, serializer: kotlinx.serialization.KSerializer<T>, value: T) {
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(json.encodeToString(serializer, value))
        tmp.renameTo(file)
    }
}

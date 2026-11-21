package io.github.danielpereod.opdshelf.data

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.SerialName
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
    /** When [finishedByHand] was set, here or on another device, in Unix seconds (server time once synced). */
    val markedAt: Long = 0,
    /** When [percentage] was read, in Unix seconds (server time once synced); 0 if unknown. */
    val positionAt: Long = 0,
    /** Left by an older build (the one of PR #54) that kept when a book was marked; only read, to migrate. */
    @SerialName("finishedAt") val legacyFinishedAt: Long = 0,
    val meta: BookMeta = BookMeta(),
) {
    /**
     * Finished, as the server counts it: marked as finished, or read to [FINISHED_AT];
     * a mark of "not finished" only counts for what was read before it.
     */
    val finished: Boolean
        get() = when (finishedByHand) {
            true -> true
            false -> percentage >= FINISHED_AT && positionAt > markedAt
            null -> percentage >= FINISHED_AT
        }

    companion object {
        /** The server's threshold, which it also sends as `finished_at`. */
        const val FINISHED_AT = 0.99
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
    // Declared first: the files are read below
    private val damagedFiles = mutableListOf<String>()
    /** Names of the files that could not be read at start (a copy of each is kept as `.bad`). */
    val damaged: List<String> get() = damagedFiles.toList()

    val booksDir = File(dir, "books").apply { mkdirs() }
    private val booksFile = File(dir, "books.json")
    private val positionsFile = File(dir, "positions.json")
    private val bookmarksFile = File(dir, "bookmarks.json")
    private val bookmarksSerializer = MapSerializer(String.serializer(), ListSerializer(Bookmark.serializer()))
    private var bookmarks: Map<String, List<Bookmark>> = read(bookmarksFile, bookmarksSerializer) ?: emptyMap()

    private val positionsSerializer = MapSerializer(String.serializer(), LocalPosition.serializer())
    private val _books = MutableStateFlow(read(booksFile, ListSerializer(LocalBook.serializer())) ?: emptyList())
    val books: StateFlow<List<LocalBook>> = _books.asStateFlow()
    private var positions: Map<String, LocalPosition> = read(positionsFile, positionsSerializer) ?: emptyMap()

    private val annotationsFile = File(dir, "annotations.json")
    private val annotationsSerializer = MapSerializer(String.serializer(), LocalAnnotations.serializer())
    private val _annotations = MutableStateFlow(read(annotationsFile, annotationsSerializer) ?: emptyMap())
    /** Highlights of each book (by document id), as they change. */
    val annotations: StateFlow<Map<String, LocalAnnotations>> = _annotations.asStateFlow()

    private val marksFile = File(dir, "marks.json")
    private val marksSerializer = MapSerializer(String.serializer(), ReadMark.serializer())
    /** Finished marks made here and not sent yet, by document id. */
    private var pendingMarks: Map<String, ReadMark> = read(marksFile, marksSerializer) ?: emptyMap()

    private val statsFile = File(dir, "stats.json")

    init {
        migrateMarks()
    }

    /**
     * Books marked as finished (or not) by an older version never had their mark
     * queued, so the server and the other devices never heard of it. Queues those
     * marks once: the book gets a [LocalBook.markedAt] that no longer matches.
     */
    private fun migrateMarks() {
        val fallback = booksFile.lastModified().takeIf { it > 0 }?.div(1000) ?: return
        val migrated = marksToMigrate(_books.value, pendingMarks, fallback)
        if (migrated.isEmpty()) return
        _books.value = _books.value.map { book ->
            migrated[book.document]?.let { book.copy(markedAt = it.at) } ?: book
        }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
        pendingMarks = pendingMarks + migrated
        write(marksFile, marksSerializer, pendingMarks)
    }

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
        // The bookmarks stay: the book can be downloaded again at any time, and they come back with it
        _books.value = _books.value.filterNot { it.id == book.id }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
    }

    /** Keeps a downloaded book tied to its catalog entry when the entry's id changes (it was moved on the server). */
    @Synchronized
    fun changeId(old: String, new: String) {
        if (old == new || book(old) == null) return
        _books.value = _books.value.map { if (it.id == old) it.copy(id = new) else it }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
        bookmarks[old]?.let { list ->
            bookmarks = bookmarks - old + (new to list)
            write(bookmarksFile, bookmarksSerializer, bookmarks)
        }
    }

    @Synchronized
    fun setPercentage(document: String, percentage: Double, at: Long = System.currentTimeMillis() / 1000) {
        _books.value = _books.value.map {
            if (it.document == document) it.copy(percentage = percentage, positionAt = at) else it
        }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
    }

    /**
     * Takes the positions of the server ([current], by document id) for the books on this
     * device, so what was read elsewhere shows here: unless this device has a position
     * not sent yet, or the server's is not newer.
     */
    @Synchronized
    fun adoptPositions(current: Map<String, Position>) {
        val next = _books.value.map { book ->
            val server = current[book.document] ?: return@map book
            if (positions[book.document]?.pending == true || server.timestamp <= book.positionAt) book
            else book.copy(percentage = server.percentage, positionAt = server.timestamp)
        }
        if (next != _books.value) {
            _books.value = next
            write(booksFile, ListSerializer(LocalBook.serializer()), next)
        }
    }

    @Synchronized
    fun markOpened(id: String) {
        _books.value = _books.value.map { if (it.id == id) it.copy(lastOpenedAt = System.currentTimeMillis()) else it }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
    }

    /**
     * Marks a book as finished or not, to be sent to the server ([pendingMarks]). "Not
     * finished" below the threshold removes the mark, so the book shows how far it was
     * read; above it, it is marked as unread.
     */
    @Synchronized
    fun setFinished(id: String, finished: Boolean, at: Long = System.currentTimeMillis() / 1000) {
        val book = book(id) ?: return
        val clear = !finished && book.percentage < LocalBook.FINISHED_AT
        val mark = if (clear) ReadMark.clear(at) else ReadMark.of(finished, at)
        _books.value = _books.value.map {
            if (it.document == book.document) it.copy(finishedByHand = if (clear) null else finished, markedAt = at) else it
        }
        write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
        pendingMarks = pendingMarks + (book.document to mark)
        write(marksFile, marksSerializer, pendingMarks)
    }

    @Synchronized
    fun pendingMarks(): Map<String, ReadMark> = pendingMarks

    /**
     * After sending [sent]: it is no longer pending (unless marked again meanwhile),
     * and the book takes the server's answer, which may be a newer mark from elsewhere.
     */
    @Synchronized
    fun markSent(document: String, sent: ReadMark, server: ReadMark?) {
        if (pendingMarks[document] != sent) return
        pendingMarks = pendingMarks - document
        write(marksFile, marksSerializer, pendingMarks)
        if (server != null) {
            _books.value = _books.value.map {
                if (it.document == document) it.copy(finishedByHand = server.finished, markedAt = server.at) else it
            }
            write(booksFile, ListSerializer(LocalBook.serializer()), _books.value)
        }
    }

    /** Applies the server's marks (by document id) to the books on this device. */
    @Synchronized
    fun adoptMarks(marks: Map<String, ReadMark>) {
        val next = _books.value.map { it.withMark(marks[it.document], it.document in pendingMarks) }
        if (next != _books.value) {
            _books.value = next
            write(booksFile, ListSerializer(LocalBook.serializer()), next)
        }
    }

    /** Drops the marks the server says were cleared, if that is newer than the book's own. */
    @Synchronized
    fun adoptCleared(cleared: Map<String, Long>) {
        val next = _books.value.map {
            val at = cleared[it.document]
            if (at != null && it.finishedByHand != null && it.document !in pendingMarks && at > it.markedAt) {
                it.copy(finishedByHand = null, markedAt = at)
            } else it
        }
        if (next != _books.value) {
            _books.value = next
            write(booksFile, ListSerializer(LocalBook.serializer()), next)
        }
    }

    /** Keeps downloaded books tied to their entries when a folder they are in moves from [from] to [to]. */
    @Synchronized
    fun folderMoved(from: String, to: String) {
        _books.value.forEach { book -> bookIdAfterFolderMove(book.id, from, to)?.let { changeId(book.id, it) } }
    }

    /** The last statistics fetched, to show offline. */
    fun cachedStats(): CachedStats? = read(statsFile, CachedStats.serializer())

    fun saveStats(stats: CachedStats) = write(statsFile, CachedStats.serializer(), stats)

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

    /**
     * Forgets everything waiting to be sent (positions, marks, highlights). For when the app is
     * connected to another server: those changes are of the old one and must not reach the new.
     * The books and highlights shown stay.
     */
    @Synchronized
    fun discardPending() {
        positions = positions.mapValues { it.value.copy(pending = false) }
        write(positionsFile, positionsSerializer, positions)
        pendingMarks = emptyMap()
        write(marksFile, marksSerializer, pendingMarks)
        _annotations.value.keys.toList().forEach { document ->
            updateAnnotations(document) { it.copy(pending = emptyMap()) }
        }
    }

    /** Highlight changes not sent yet, by book. */
    @Synchronized
    fun pendingAnnotations(): Map<String, List<Annotation>> =
        _annotations.value.mapValues { it.value.pending.values.toList() }.filterValues { it.isNotEmpty() }

    /**
     * The file's content, or null if there is none. A file that exists but cannot be read
     * (a power cut while it was written, a bad update) is copied aside as `.bad` before the
     * next write replaces it, instead of being taken for an empty list and overwritten.
     */
    private fun <T> read(file: File, serializer: kotlinx.serialization.KSerializer<T>): T? {
        if (!file.exists()) return null
        return runCatching { json.decodeFromString(serializer, file.readText()) }.getOrElse {
            runCatching { file.copyTo(File(file.parentFile, file.name + ".bad"), overwrite = true) }
            damagedFiles += file.name
            null
        }
    }

    /** Writes to a temporary file, makes sure it reached the disk, and only then replaces the real one. */
    private fun <T> write(file: File, serializer: kotlinx.serialization.KSerializer<T>, value: T) {
        val tmp = File(file.parentFile, file.name + ".tmp")
        java.io.FileOutputStream(tmp).use { out ->
            out.write(json.encodeToString(serializer, value).toByteArray())
            out.fd.sync()
        }
        if (!tmp.renameTo(file)) {
            tmp.copyTo(file, overwrite = true)
            tmp.delete()
        }
    }
}

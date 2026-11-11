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
)

/** Downloaded books and reading positions, kept as JSON files in the app's storage. */
class LocalStore(private val dir: File) {
    val booksDir = File(dir, "books").apply { mkdirs() }
    private val booksFile = File(dir, "books.json")
    private val positionsFile = File(dir, "positions.json")

    private val positionsSerializer = MapSerializer(String.serializer(), LocalPosition.serializer())
    private val _books = MutableStateFlow(read(booksFile, ListSerializer(LocalBook.serializer())) ?: emptyList())
    val books: StateFlow<List<LocalBook>> = _books.asStateFlow()
    private var positions: Map<String, LocalPosition> = read(positionsFile, positionsSerializer) ?: emptyMap()

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

    private fun <T> read(file: File, serializer: kotlinx.serialization.KSerializer<T>): T? =
        runCatching { json.decodeFromString(serializer, file.readText()) }.getOrNull()

    private fun <T> write(file: File, serializer: kotlinx.serialization.KSerializer<T>, value: T) {
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(json.encodeToString(serializer, value))
        tmp.renameTo(file)
    }
}

package io.github.danielpereod.opdshelf.ui

import android.app.Application
import androidx.annotation.StringRes
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.Entry
import io.github.danielpereod.opdshelf.data.Feed
import io.github.danielpereod.opdshelf.data.LocalBook
import io.github.danielpereod.opdshelf.data.Position
import io.github.danielpereod.opdshelf.data.partialMd5
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.net.URLEncoder
import java.security.MessageDigest

data class FeedState(
    val url: String,
    /** Shown instead of the feed's own title, e.g. for search results. */
    val label: String? = null,
    val feed: Feed? = null,
    val loading: Boolean = true,
    val loadingMore: Boolean = false,
    val error: String? = null,
)

/** The ways into the catalog offered above it, as paths of OPDShelf's OPDS feeds. */
enum class Section(@StringRes val label: Int, val path: String) {
    Recent(R.string.section_recent, "/opds/books"),
    Title(R.string.section_title, "/opds/books?sort=name-asc"),
    Authors(R.string.section_authors, "/opds/authors"),
    Series(R.string.section_series, "/opds/series"),
    Folders(R.string.section_folders, "/opds/folders"),
}

/** A message under the screen, with what its button does: read the new book, or try again. */
data class Notice(
    val text: String,
    @StringRes val action: Int? = null,
    val book: LocalBook? = null,
    val retry: Entry? = null,
)

/** The book whose details are open: from the catalog, or only on this device. */
data class Detail(val entry: Entry?, val bookId: String)

class LibraryViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application.app

    private val _section = MutableStateFlow(Section.Recent)
    val section: StateFlow<Section> = _section.asStateFlow()

    /** The feeds opened inside the section, the last one on screen. */
    private val _stack = MutableStateFlow<List<FeedState>>(emptyList())
    val stack: StateFlow<List<FeedState>> = _stack.asStateFlow()

    /** Download progress by entry id, from 0 to 1. */
    private val _downloads = MutableStateFlow<Map<String, Float>>(emptyMap())
    val downloads: StateFlow<Map<String, Float>> = _downloads.asStateFlow()

    private val _messages = MutableStateFlow<Notice?>(null)
    val messages: StateFlow<Notice?> = _messages.asStateFlow()

    /** Running downloads, to cancel them. */
    private val jobs = mutableMapOf<String, kotlinx.coroutines.Job>()

    private val _detail = MutableStateFlow<Detail?>(null)
    val detail: StateFlow<Detail?> = _detail.asStateFlow()

    /** The server's latest position of the book in the details, to say where it was left. */
    private val _remote = MutableStateFlow<Position?>(null)
    val remote: StateFlow<Position?> = _remote.asStateFlow()

    val books = app.store.books
    val syncStatus = app.sync.status

    private val text get() = getApplication<Application>()

    fun openRoot() = selectSection(_section.value)

    fun selectSection(section: Section) {
        if (!app.settings.isConfigured) return
        _section.value = section
        _stack.value = emptyList()
        open(app.api.url(section.path))
    }

    fun open(url: String, label: String? = null) {
        _stack.value = _stack.value + FeedState(url, label)
        load(url)
    }

    fun search(query: String) {
        val template = _stack.value.firstOrNull()?.feed?.searchUrl ?: app.api.url("/opds/search?q={searchTerms}")
        // A new search replaces the previous results instead of piling up
        if (_stack.value.lastOrNull()?.label != null && _stack.value.size > 1) _stack.value = _stack.value.dropLast(1)
        open(template.replace("{searchTerms}", URLEncoder.encode(query, "UTF-8")), text.getString(R.string.results_for, query))
    }

    fun back(): Boolean {
        if (_stack.value.size <= 1) return false
        _stack.value = _stack.value.dropLast(1)
        return true
    }

    fun retry() {
        _stack.value.lastOrNull()?.let { load(it.url) }
    }

    private fun load(url: String) {
        update(url) { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                val feed = app.api.feed(url)
                update(url) { it.copy(feed = feed, loading = false) }
            } catch (e: Exception) {
                update(url) { it.copy(loading = false, error = text.describe(e)) }
            }
        }
    }

    /** Appends the next page of the current feed. */
    fun loadMore() {
        val state = _stack.value.lastOrNull() ?: return
        val next = state.feed?.nextUrl ?: return
        if (state.loadingMore) return
        update(state.url) { it.copy(loadingMore = true) }
        viewModelScope.launch {
            try {
                val page = app.api.feed(next)
                update(state.url) {
                    it.copy(feed = it.feed?.copy(entries = it.feed.entries + page.entries, nextUrl = page.nextUrl), loadingMore = false)
                }
            } catch (e: Exception) {
                update(state.url) { it.copy(loadingMore = false) }
                _messages.value = Notice(text.describe(e))
            }
        }
    }

    private fun update(url: String, change: (FeedState) -> FeedState) {
        _stack.value = _stack.value.map { if (it.url == url) change(it) else it }
    }

    // -----------------------------------------------------------------------
    // Book details

    fun showDetail(entry: Entry?, bookId: String) {
        _detail.value = Detail(entry, bookId)
        _remote.value = null
        val book = app.store.book(bookId) ?: return
        viewModelScope.launch {
            _remote.value = runCatching { app.api.getProgress(book.document) }.getOrNull()
        }
    }

    fun closeDetail() {
        _detail.value = null
    }

    // -----------------------------------------------------------------------
    // Downloads

    fun download(entry: Entry, onDone: (LocalBook) -> Unit = {}) {
        val acquisition = entry.acquisition ?: return
        if (_downloads.value.containsKey(entry.id)) return
        _downloads.value = _downloads.value + (entry.id to 0f)
        jobs[entry.id] = viewModelScope.launch {
            try {
                val base = sha1(entry.id).take(16)
                val ext = extension(acquisition.url, acquisition.type)
                val file = File(app.store.booksDir, "$base.$ext")
                app.api.download(acquisition.url, file) { p -> _downloads.value = _downloads.value + (entry.id to p) }
                val cover = (entry.coverUrl ?: entry.thumbnailUrl)?.let { url ->
                    runCatching {
                        val coverFile = File(app.store.booksDir, "$base.cover")
                        app.api.download(url, coverFile) {}
                        coverFile.name
                    }.getOrNull()
                }
                val document = withContext(Dispatchers.IO) { partialMd5(file) }
                val book = LocalBook(
                    id = entry.id,
                    title = entry.title,
                    authors = entry.authors,
                    fileName = file.name,
                    mediaType = acquisition.type,
                    document = document,
                    coverFile = cover,
                    summary = entry.summary,
                    format = acquisition.format ?: ext.uppercase(),
                    sizeBytes = file.length(),
                    meta = entry.meta,
                )
                app.store.addBook(book)
                // Where it was left on other devices
                runCatching { app.sync.start(document) }
                if (_detail.value?.bookId == book.id) showDetail(entry, book.id)
                _messages.value = Notice(text.getString(R.string.downloaded, book.title), R.string.read, book = book)
                onDone(book)
            } catch (e: kotlinx.coroutines.CancellationException) {
                app.store.booksDir.listFiles()?.filter { it.name.startsWith(sha1(entry.id).take(16)) }?.forEach { it.delete() }
                _messages.value = Notice(text.getString(R.string.download_cancelled))
                throw e
            } catch (e: Exception) {
                _messages.value = Notice(text.describe(e), R.string.retry, retry = entry)
            } finally {
                _downloads.value = _downloads.value - entry.id
                jobs.remove(entry.id)
            }
        }
    }

    fun setFinished(book: LocalBook, finished: Boolean) = app.store.setFinished(book.id, finished)

    fun cancelDownload(id: String) {
        jobs[id]?.cancel()
    }

    fun delete(book: LocalBook) {
        app.store.removeBook(book)
        _messages.value = Notice(text.getString(R.string.removed, book.title))
    }

    fun delete(ids: Collection<String>) {
        val gone = books.value.filter { it.id in ids }
        gone.forEach { app.store.removeBook(it) }
        _messages.value = Notice(text.resources.getQuantityString(R.plurals.removed_count, gone.size, gone.size))
    }

    fun setFinished(ids: Collection<String>, finished: Boolean) = ids.forEach { app.store.setFinished(it, finished) }

    fun deleteAll() {
        books.value.forEach { app.store.removeBook(it) }
    }

    fun syncNow() {
        viewModelScope.launch { app.sync.flush() }
    }

    fun messageShown() {
        _messages.value = null
    }

    private fun sha1(value: String): String =
        MessageDigest.getInstance("SHA-1").digest(value.toByteArray()).joinToString("") { "%02x".format(it) }

    private fun extension(url: String, type: String): String {
        val fromUrl = url.substringBefore('?').substringAfterLast('/').substringAfterLast('.', "").lowercase()
        if (fromUrl.isNotEmpty() && fromUrl.length <= 5) return fromUrl
        return when (type.substringBefore(';')) {
            "application/epub+zip" -> "epub"
            "application/pdf" -> "pdf"
            "application/vnd.comicbook+zip", "application/x-cbz" -> "cbz"
            else -> "bin"
        }
    }
}

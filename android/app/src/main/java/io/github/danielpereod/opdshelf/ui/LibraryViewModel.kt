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
import io.github.danielpereod.opdshelf.data.ServerFolder
import io.github.danielpereod.opdshelf.data.movedTo
import io.github.danielpereod.opdshelf.data.serverPath
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
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

/** A file chosen on this device to add to the server. */
data class UploadFile(
    val uri: android.net.Uri,
    val name: String,
    val size: Long,
    /** From 0 to 1 while it goes up. */
    val progress: Float? = null,
    val done: Boolean = false,
    val error: String? = null,
) {
    val supported get() = isBookFile(name)
}

/** Books on their way to the server, and the folder they go to ("" for the top). */
data class Upload(val files: List<UploadFile>, val folder: String, val running: Boolean = false, val finished: Boolean = false)

/** The formats the server takes by default, so other files are pointed out before uploading. */
private val bookExtensions = setOf(
    "epub", "kepub", "pdf", "fb2", "fbz", "mobi", "azw", "azw3", "azw4", "cbz", "cbr", "cb7", "cbt", "djvu", "rtf",
    "txt", "doc", "docx", "odt", "zip",
)

fun isBookFile(name: String): Boolean {
    val lower = name.lowercase()
    return lower.endsWith(".fb2.zip") || lower.substringAfterLast('.', "") in bookExtensions
}

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

    private val _upload = MutableStateFlow<Upload?>(null)
    val upload: StateFlow<Upload?> = _upload.asStateFlow()

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

    // -----------------------------------------------------------------------
    // Adding books to the server

    /** Opens the upload sheet for files picked here or shared from another app. */
    fun prepareUpload(uris: List<android.net.Uri>) {
        if (uris.isEmpty() || _upload.value?.running == true) return
        viewModelScope.launch {
            val files = withContext(Dispatchers.IO) { uris.distinct().map { describeFile(it) } }
            _upload.value = Upload(files, currentFolder() ?: "")
        }
    }

    private fun describeFile(uri: android.net.Uri): UploadFile {
        var name: String? = null
        var size = -1L
        runCatching {
            text.contentResolver.query(uri, null, null, null, null)?.use { c ->
                if (c.moveToFirst()) {
                    val n = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                    val s = c.getColumnIndex(android.provider.OpenableColumns.SIZE)
                    if (n >= 0) name = c.getString(n)
                    if (s >= 0 && !c.isNull(s)) size = c.getLong(s)
                }
            }
        }
        val fallback = uri.lastPathSegment?.substringAfterLast('/') ?: "book"
        return UploadFile(uri, (name ?: fallback).replace('/', '_'), size)
    }

    fun setUploadFolder(folder: String) {
        _upload.value = _upload.value?.copy(folder = folder)
    }

    fun closeUpload() {
        if (_upload.value?.running != true) _upload.value = null
    }

    fun startUpload() {
        val upload = _upload.value ?: return
        if (upload.running) return
        _upload.value = upload.copy(running = true, finished = false)
        viewModelScope.launch {
            var saved = 0
            for (file in upload.files) {
                if (!file.supported || file.done) continue
                fun change(f: (UploadFile) -> UploadFile) {
                    _upload.value = _upload.value?.let { u -> u.copy(files = u.files.map { if (it.uri == file.uri) f(it) else it }) }
                }
                change { it.copy(progress = 0f, error = null) }
                try {
                    app.api.upload(
                        upload.folder, file.name, file.size,
                        open = { text.contentResolver.openInputStream(file.uri) ?: throw java.io.IOException(text.getString(R.string.upload_unreadable)) },
                        onProgress = { p -> change { it.copy(progress = p) } },
                    )
                    saved++
                    change { it.copy(progress = null, done = true) }
                } catch (e: kotlinx.coroutines.CancellationException) {
                    throw e
                } catch (e: Exception) {
                    change { it.copy(progress = null, error = text.describe(e)) }
                }
            }
            val now = _upload.value ?: return@launch
            val failed = now.files.any { it.supported && !it.done }
            // Show the new books where the catalog is open
            _stack.value.lastOrNull()?.let { load(it.url) }
            if (failed) {
                _upload.value = now.copy(running = false, finished = true)
            } else {
                _upload.value = null
                val where = upload.folder.ifEmpty { text.getString(R.string.library_top) }
                _messages.value = Notice(text.resources.getQuantityString(R.plurals.uploaded_count, saved, saved, where))
            }
        }
    }

    fun setFinished(book: LocalBook, finished: Boolean) = app.store.setFinished(book.id, finished)

    // -----------------------------------------------------------------------
    // Folders on the server

    /** The library's folders, for choosing where a book goes; null while they load. */
    private val _folders = MutableStateFlow<List<ServerFolder>?>(null)
    val folders: StateFlow<List<ServerFolder>?> = _folders.asStateFlow()

    fun loadFolders() {
        _folders.value = null
        viewModelScope.launch {
            _folders.value = try {
                app.api.folders()
            } catch (e: Exception) {
                _messages.value = Notice(text.describe(e))
                emptyList()
            }
        }
    }

    /** The folder open in the Folders section, "" at its top; null anywhere else. */
    fun currentFolder(stack: List<FeedState> = _stack.value): String? {
        val state = stack.lastOrNull() ?: return null
        if (_section.value != Section.Folders || state.label != null) return null
        val segments = state.url.toHttpUrlOrNull()?.pathSegments ?: return null
        val at = segments.indexOf("folders")
        if (at < 0 || segments.getOrNull(at - 1) != "opds") return null
        return segments.drop(at + 1).filter { it.isNotEmpty() }.joinToString("/")
    }

    /** Creates a folder inside the one open, or at the top. */
    fun createFolder(name: String) {
        val parent = currentFolder() ?: ""
        viewModelScope.launch {
            try {
                val path = app.api.createFolder(parent, name.trim())
                _messages.value = Notice(text.getString(R.string.folder_created, path))
                reloadAll()
            } catch (e: Exception) {
                _messages.value = Notice(text.describe(e))
            }
        }
    }

    /** Moves a book of the catalog into [folder] ("" for the top), creating the folder if it is new. */
    fun moveToFolder(entry: Entry, folder: String) {
        val path = entry.serverPath ?: return
        val target = folder.trim().trim('/')
        viewModelScope.launch {
            try {
                val result = app.api.moveBooks(listOf(path), target)
                val newPath = listOf(result.folder, path.substringAfterLast('/')).filter { it.isNotEmpty() }.joinToString("/")
                val moved = entry.movedTo(path, newPath)
                // The copy on this device stays tied to the book
                app.store.changeId(entry.id, moved.id)
                if (_detail.value?.bookId == entry.id) _detail.value = Detail(moved, moved.id)
                _messages.value = Notice(
                    if (result.folder.isEmpty()) text.getString(R.string.moved_to_top, entry.title)
                    else text.getString(R.string.moved_to_folder, entry.title, result.folder)
                )
                reloadAll()
            } catch (e: Exception) {
                _messages.value = Notice(text.describe(e))
            }
        }
    }

    /** Folder counts and contents change after a move, so every open feed is read again. */
    private fun reloadAll() = _stack.value.forEach { load(it.url) }

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

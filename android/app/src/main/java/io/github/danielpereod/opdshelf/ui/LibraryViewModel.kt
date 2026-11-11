package io.github.danielpereod.opdshelf.ui

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.Entry
import io.github.danielpereod.opdshelf.data.Feed
import io.github.danielpereod.opdshelf.data.LocalBook
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
    val feed: Feed? = null,
    val loading: Boolean = true,
    val loadingMore: Boolean = false,
    val error: String? = null,
)

class LibraryViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application.app

    /** The feeds opened, the last one on screen. */
    private val _stack = MutableStateFlow<List<FeedState>>(emptyList())
    val stack: StateFlow<List<FeedState>> = _stack.asStateFlow()

    /** Download progress by entry id, from 0 to 1. */
    private val _downloads = MutableStateFlow<Map<String, Float>>(emptyMap())
    val downloads: StateFlow<Map<String, Float>> = _downloads.asStateFlow()

    private val _messages = MutableStateFlow<String?>(null)
    val messages: StateFlow<String?> = _messages.asStateFlow()

    val books = app.store.books

    fun openRoot() {
        if (!app.settings.isConfigured) return
        _stack.value = emptyList()
        open(app.api.url("/"))
    }

    fun open(url: String) {
        _stack.value = _stack.value + FeedState(url)
        load(url)
    }

    fun search(query: String) {
        val template = _stack.value.lastOrNull()?.feed?.searchUrl ?: app.api.url("/opds/search?q={searchTerms}")
        open(template.replace("{searchTerms}", URLEncoder.encode(query, "UTF-8")))
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
                update(url) { it.copy(loading = false, error = getApplication<Application>().describe(e)) }
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
                _messages.value = getApplication<Application>().describe(e)
            }
        }
    }

    private fun update(url: String, change: (FeedState) -> FeedState) {
        _stack.value = _stack.value.map { if (it.url == url) change(it) else it }
    }

    fun download(entry: Entry, onDone: (LocalBook) -> Unit = {}) {
        val acquisition = entry.acquisition ?: return
        if (_downloads.value.containsKey(entry.id)) return
        _downloads.value = _downloads.value + (entry.id to 0f)
        viewModelScope.launch {
            try {
                val base = sha1(entry.id).take(16)
                val ext = extension(acquisition.url, acquisition.type)
                val file = File(app.store.booksDir, "$base.$ext")
                app.api.download(acquisition.url, file) { p -> _downloads.value = _downloads.value + (entry.id to p) }
                val cover = (entry.thumbnailUrl ?: entry.coverUrl)?.let { url ->
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
                )
                app.store.addBook(book)
                // Show how far it was read elsewhere
                runCatching { app.sync.start(document) }
                onDone(book)
            } catch (e: Exception) {
                _messages.value = getApplication<Application>().describe(e)
            } finally {
                _downloads.value = _downloads.value - entry.id
            }
        }
    }

    fun delete(book: LocalBook) = app.store.removeBook(book)

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

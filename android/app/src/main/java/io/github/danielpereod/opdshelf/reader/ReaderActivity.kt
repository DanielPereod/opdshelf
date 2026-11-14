package io.github.danielpereod.opdshelf.reader

import android.content.ClipData
import android.content.ClipboardManager
import android.os.Bundle
import android.view.ActionMode
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.Toast
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.fragment.app.FragmentContainerView
import androidx.fragment.app.commitNow
import androidx.lifecycle.lifecycleScope
import com.google.android.material.snackbar.Snackbar
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.LocalBook
import io.github.danielpereod.opdshelf.data.Position
import io.github.danielpereod.opdshelf.data.PositionContext
import io.github.danielpereod.opdshelf.data.json
import io.github.danielpereod.opdshelf.ui.AppTheme
import io.github.danielpereod.opdshelf.data.Bookmark
import android.view.KeyEvent
import org.readium.r2.shared.publication.services.search.isSearchable
import org.readium.r2.shared.publication.services.search.search
import io.github.danielpereod.opdshelf.ui.readerPalette
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import org.json.JSONObject
import org.readium.adapter.pdfium.navigator.PdfiumEngineProvider
import org.readium.r2.navigator.VisualNavigator
import org.readium.r2.navigator.OverflowableNavigator
import org.readium.r2.navigator.epub.EpubNavigatorFactory
import org.readium.r2.navigator.epub.EpubNavigatorFragment
import org.readium.r2.navigator.epub.EpubPreferences
import org.readium.r2.navigator.image.ImageNavigatorFragment
import org.readium.r2.navigator.input.InputListener
import org.readium.r2.navigator.input.TapEvent
import org.readium.r2.navigator.pdf.PdfNavigatorFactory
import org.readium.r2.navigator.pdf.PdfNavigatorFragment
import org.readium.r2.navigator.preferences.FontFamily
import org.readium.r2.navigator.preferences.Theme
import org.readium.r2.navigator.util.DirectionalNavigationAdapter
import org.readium.r2.shared.publication.Link
import org.readium.r2.shared.publication.Locator
import org.readium.r2.shared.publication.Publication
import org.readium.r2.shared.publication.indexOfFirstWithHref
import org.readium.r2.shared.publication.services.locateProgression
import org.readium.r2.shared.publication.services.positions
import org.readium.r2.shared.util.getOrElse
import org.readium.r2.shared.util.mediatype.MediaType

/** What the bridge script returns for the selected text. */
@Serializable
private data class SelectedRange(
    val pos0: String,
    val pos1: String,
    val text: String = "",
    val context: PositionContext = PositionContext(),
)

/** What the bridge script returns for a point of an EPUB chapter. */
@Serializable
private data class Point(val progress: String, val context: PositionContext = PositionContext())

private enum class Kind { EPUB, PDF, IMAGES }

/**
 * Reads a downloaded book with Readium and keeps its position in sync.
 *
 * EPUB positions are KOReader xpointers, worked out inside the page by the
 * same script the web reader uses, so KOReader, the web and this app agree on
 * the exact sentence. PDFs and comics use page numbers.
 */
class ReaderActivity : AppCompatActivity() {
    companion object {
        const val EXTRA_BOOK_ID = "book_id"
        /** A highlight to open the book at. */
        const val EXTRA_ANNOTATION = "annotation"
        private const val HIGHLIGHTS = "highlights"
        private const val HIGHLIGHT_ITEM = 3
        private const val NOTE_ITEM = 4
        private const val TAG = "navigator"
        private const val MARK_ITEM = 1
        private const val COPY_ITEM = 2
    }

    private lateinit var book: LocalBook
    private lateinit var container: FragmentContainerView
    private var publication: Publication? = null
    private var positionCount = 0
    private var kind = Kind.EPUB
    private var navigator: VisualNavigator? = null
    private var scripts: String? = null

    /** Positions are saved only once reading moves away from where the book opened. */
    private var ready = false
    private var baseline: Locator? = null

    private val prefs by lazy { getSharedPreferences("reader", MODE_PRIVATE) }
    private val ui = ReaderUi()
    private lateinit var root: FrameLayout

    override fun onCreate(savedInstanceState: Bundle?) {
        // The navigator needs the opened publication, so it is not restored by the system
        super.onCreate(null)
        val found = intent.getStringExtra(EXTRA_BOOK_ID)?.let { app.store.book(it) }
        if (found == null) {
            finish()
            return
        }
        book = found
        title = book.title
        app.store.markOpened(book.id)
        ui.title = book.title
        ui.prefs = loadPrefs()
        ui.bookmarks = app.store.bookmarks(book.id)
        applyScreenOn()
        applyBrightness()
        WindowCompat.setDecorFitsSystemWindows(window, false)
        container = FragmentContainerView(this).apply { id = View.generateViewId() }
        val overlay = ComposeView(this).apply {
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnViewTreeLifecycleDestroyed)
            // The bars take the page's colors (light, sepia or dark), not the system's
            setContent { AppTheme(readerPalette(ui.prefs.theme)) { ReaderOverlay(ui, actions) } }
        }
        root = FrameLayout(this).apply {
            addView(container, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            addView(overlay, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        }
        setContentView(root)
        onBackPressedDispatcher.addCallback(this, object : androidx.activity.OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                when {
                    ui.sheet != null -> ui.sheet = null
                    ui.visible -> showBars(false)
                    else -> finish()
                }
            }
        })
        lifecycleScope.launch { open() }
    }

    override fun onDestroy() {
        super.onDestroy()
        publication?.close()
    }

    private suspend fun open() {
        val asset = app.assetRetriever.retrieve(app.store.file(book)).getOrElse {
            fail()
            return
        }
        val pub = app.publicationOpener.open(asset, allowUserInteraction = false).getOrElse {
            asset.close()
            fail()
            return
        }
        publication = pub
        kind = when {
            pub.conformsTo(Publication.Profile.EPUB) -> Kind.EPUB
            pub.conformsTo(Publication.Profile.PDF) -> Kind.PDF
            else -> Kind.IMAGES
        }
        val start = app.sync.start(book.document)
        val exact = start.locator?.let { runCatching { Locator.fromJSON(JSONObject(it)) }.getOrNull() }
        val xpointerStart = start.position?.takeIf { exact == null && kind == Kind.EPUB && it.progress.startsWith("/") }
        val initial = exact ?: initialLocator(pub, start.position)

        val factory = when (kind) {
            Kind.EPUB -> EpubNavigatorFactory(pub).createFragmentFactory(
                initialLocator = initial,
                initialPreferences = epubPreferences(),
                configuration = EpubNavigatorFragment.Configuration {
                    selectionActionModeCallback = selectionCallback
                },
            )
            Kind.PDF -> PdfNavigatorFactory(pub, PdfiumEngineProvider()).createFragmentFactory(initialLocator = initial)
            Kind.IMAGES -> ImageNavigatorFragment.createFactory(pub, initial)
        }
        supportFragmentManager.fragmentFactory = factory
        val fragmentClass = when (kind) {
            Kind.EPUB -> EpubNavigatorFragment::class.java
            Kind.PDF -> PdfNavigatorFragment::class.java
            Kind.IMAGES -> ImageNavigatorFragment::class.java
        }
        supportFragmentManager.commitNow { add(container.id, fragmentClass, null, TAG) }
        val nav = supportFragmentManager.findFragmentByTag(TAG) as VisualNavigator
        navigator = nav
        (nav as? OverflowableNavigator)?.let { nav.addInputListener(DirectionalNavigationAdapter(it)) }
        nav.addInputListener(object : InputListener {
            override fun onTap(event: TapEvent): Boolean {
                toggleBars()
                return true
            }
        })
        showBars(false)
        ui.reflowable = kind == Kind.EPUB
        ui.toc = flatten(pub.tableOfContents)
        ui.searchable = pub.isSearchable
        val positions = pub.positions()
        positionsCache = positions
        positionCount = positions.size
        ui.pages = positions.size.takeIf { it > 0 }
        ui.showHint = kind == Kind.EPUB && !app.settings.markHintShown
        lifecycleScope.launch {
            nav.currentLocator.collect { locator ->
                // Each chapter is its own web page: our script (which also steadies text selection) goes into
                // the one on screen; once there, this only checks that it is
                if (kind == Kind.EPUB) launch { js("0") }
                ui.chapter = locator.title ?: chapterOf(locator.href.toString())
                ui.progression = (locator.locations.totalProgression ?: 0.0).toFloat().coerceIn(0f, 1f)
                ui.page = locator.locations.position
                ui.currentHref = locator.href.toString()
                ui.currentBookmark = ui.bookmarks.firstOrNull { onPage(it, locator) }
                // About 0.75 minutes per Readium position (some 1024 characters) left in this chapter
                ui.chapterMinutes = if (kind == Kind.EPUB) locator.locations.position?.let { at ->
                    val left = positions.count { it.href == locator.href && (it.locations.position ?: 0) > at }
                    (left * 0.75).toInt()
                } else null
            }
        }

        if (xpointerStart != null) goToXPointer(pub, xpointerStart, highlight = xpointerStart.manual && start.fromDevice != null)
        start.fromDevice?.let {
            Snackbar.make(root, getString(R.string.continuing_from, it), Snackbar.LENGTH_LONG).show()
        }
        watchAnnotations()
        intent.getStringExtra(EXTRA_ANNOTATION)?.let { id ->
            app.sync.refreshAnnotations(book.document)
            app.store.annotations(book.document).find(id)?.takeUnless { it.deleted }?.let { goToAnnotation(it) }
        }
        delay(800)
        baseline = nav.currentLocator.value
        ready = true
        watchPosition(nav)
    }

    // -----------------------------------------------------------------------
    // Highlights and notes, shared with the web reader and KOReader

    private fun watchAnnotations() {
        (navigator as? org.readium.r2.navigator.DecorableNavigator)?.addDecorationListener(
            HIGHLIGHTS,
            object : org.readium.r2.navigator.DecorableNavigator.Listener {
                override fun onDecorationActivated(event: org.readium.r2.navigator.DecorableNavigator.OnActivatedEvent): Boolean {
                    val a = ui.annotations.firstOrNull { it.id == event.decoration.id } ?: return false
                    ui.editingNew = false
                    ui.editing = a
                    return true
                }
            },
        )
        lifecycleScope.launch {
            app.store.annotations
                .map { it[book.document]?.visible().orEmpty() }
                .distinctUntilChanged()
                .collect { list ->
                    ui.annotations = list
                    decorate(list)
                }
        }
        // Highlights made on other devices: when opening, and every little while
        lifecycleScope.launch {
            repeatOnLifecycle(androidx.lifecycle.Lifecycle.State.STARTED) {
                while (true) {
                    app.sync.refreshAnnotations(book.document)
                    delay(30_000)
                }
            }
        }
    }

    /** Where a highlight is, for Readium: its chapter and its words, found in the page by the text around them. */
    private fun locatorOf(a: io.github.danielpereod.opdshelf.data.Annotation): Locator? {
        val pub = publication ?: return null
        if (kind != Kind.EPUB) {
            val page = a.page ?: a.pos0.toIntOrNull() ?: return null
            return positionsCache?.getOrNull(page - 1)
        }
        val link = pub.readingOrder.getOrNull(spineIndex(a.pos0)) ?: return null
        return Locator(
            href = link.url(),
            mediaType = link.mediaType ?: MediaType.XHTML,
            locations = Locator.Locations(totalProgression = a.percentage),
            text = Locator.Text(before = a.context?.before, highlight = a.text.ifBlank { null }, after = a.context?.after),
        )
    }

    private var positionsCache: List<Locator>? = null

    private suspend fun decorate(list: List<io.github.danielpereod.opdshelf.data.Annotation>) {
        val nav = navigator as? EpubNavigatorFragment ?: return
        val decorations = list.mapNotNull { a ->
            val locator = locatorOf(a)?.takeIf { a.text.isNotBlank() } ?: return@mapNotNull null
            val tint = io.github.danielpereod.opdshelf.data.Annotation.argb(a.color)
            val style = if (a.style == "underscore" || a.style == "strikeout") org.readium.r2.navigator.Decoration.Style.Underline(tint)
            else org.readium.r2.navigator.Decoration.Style.Highlight(tint)
            org.readium.r2.navigator.Decoration(id = a.id, locator = locator, style = style)
        }
        nav.applyDecorations(decorations, HIGHLIGHTS)
    }

    private fun goToAnnotation(a: io.github.danielpereod.opdshelf.data.Annotation) {
        val locator = locatorOf(a) ?: return
        navigator?.go(locator)
    }

    private suspend fun highlightSelection(withNote: Boolean) {
        val epub = navigator as? EpubNavigatorFragment ?: return
        val selection = epub.currentSelection() ?: return
        val locator = selection.locator
        val index = publication?.readingOrder?.indexOfFirstWithHref(locator.href) ?: return
        val range = js("OPDShelf.selectionRange($index)")
            ?.let { runCatching { json.decodeFromString<SelectedRange>(it) }.getOrNull() } ?: return
        epub.clearSelection()
        val annotation = io.github.danielpereod.opdshelf.data.Annotation(
            id = io.github.danielpereod.opdshelf.data.Annotation.newId(),
            text = range.text.ifBlank { locator.text.highlight.orEmpty() }.take(8000),
            color = prefs.getString("highlight_color", "yellow") ?: "yellow",
            pos0 = range.pos0,
            pos1 = range.pos1,
            context = range.context,
            chapter = ui.chapter.orEmpty(),
            percentage = (locator.locations.totalProgression ?: ui.progression.toDouble()).coerceIn(0.0, 1.0),
        )
        if (withNote) {
            ui.editingNew = true
            ui.editing = annotation
        } else {
            app.sync.annotate(book.document, annotation)
            Snackbar.make(root, R.string.highlight_added, Snackbar.LENGTH_SHORT)
                .setAction(R.string.add_note) { ui.editingNew = false; ui.editing = app.store.annotations(book.document).find(annotation.id) }
                .show()
        }
    }

    private fun fail() {
        Toast.makeText(this, R.string.cannot_open_book, Toast.LENGTH_LONG).show()
        finish()
    }

    /** Where to start when there is no exact locator saved on this device. */
    private suspend fun initialLocator(pub: Publication, position: Position?): Locator? {
        position ?: return null
        if (kind == Kind.EPUB) {
            // Opened at the chapter; the exact point is found once it is shown
            val link = pub.readingOrder.getOrNull(spineIndex(position.progress))
            if (link != null) {
                return Locator(
                    href = link.url(),
                    mediaType = link.mediaType ?: MediaType.XHTML,
                    locations = Locator.Locations(progression = 0.0),
                )
            }
        } else {
            val page = position.progress.toIntOrNull()
            pub.positions().getOrNull((page ?: 0) - 1)?.let { return it }
        }
        return pub.locateProgression(position.percentage)
    }

    private suspend fun goToXPointer(pub: Publication, position: Position, highlight: Boolean) {
        val epub = navigator as? EpubNavigatorFragment ?: return
        val link = pub.readingOrder.getOrNull(spineIndex(position.progress)) ?: return
        withTimeoutOrNull(5000) { epub.currentLocator.first { it.href == link.url() } } ?: return
        delay(300)
        val call = "OPDShelf.progression(${quote(position.progress)}, ${contextJson(position.context)}, $scroll)"
        val progression = js(call)?.toDoubleOrNull() ?: return
        if (progression < 0) return
        epub.go(
            Locator(
                href = link.url(),
                mediaType = link.mediaType ?: MediaType.XHTML,
                locations = Locator.Locations(progression = progression),
            )
        )
        if (highlight) {
            delay(500)
            js("OPDShelf.highlight(${quote(position.progress)}, ${contextJson(position.context)})")
        }
    }

    @OptIn(FlowPreview::class)
    private fun watchPosition(nav: VisualNavigator) {
        lifecycleScope.launch {
            nav.currentLocator.debounce(700).collect { locator ->
                if (ready && !sameSpot(locator, baseline)) {
                    baseline = null
                    save(locator)
                }
            }
        }
    }

    private fun sameSpot(a: Locator, b: Locator?): Boolean =
        b != null && a.href == b.href && a.locations.progression == b.locations.progression &&
            a.locations.position == b.locations.position

    /**
     * Whether the last page of the book is on screen. Readium's totalProgression is where the
     * page starts, so the last page never reaches 100 %: a finished book would stay at 95-99 %.
     */
    private suspend fun atEnd(locator: Locator): Boolean {
        val pub = publication ?: return false
        val position = locator.locations.position
        if (kind != Kind.EPUB) return position != null && positionCount > 0 && position >= positionCount
        if (pub.readingOrder.indexOfFirstWithHref(locator.href) != pub.readingOrder.lastIndex) return false
        // In the last chapter: ask the page whether its end is visible
        val end = js(
            "(function(){var e=document.scrollingElement;" +
                "return Math.abs(e.scrollLeft)+window.innerWidth>=e.scrollWidth-2&&e.scrollTop+window.innerHeight>=e.scrollHeight-2})()"
        )
        return end == "true"
    }

    private suspend fun save(locator: Locator) {
        val percentage = if (atEnd(locator)) 1.0 else (locator.locations.totalProgression ?: 0.0).coerceIn(0.0, 1.0)
        if (percentage == 1.0) ui.progression = 1f
        val position = when (kind) {
            Kind.EPUB -> {
                val index = publication?.readingOrder?.indexOfFirstWithHref(locator.href) ?: return
                val point = js("OPDShelf.firstVisible($index)")?.let { decodePoint(it) } ?: return
                Position(progress = point.progress, percentage = percentage, context = point.context)
            }
            else -> Position(progress = (locator.locations.position ?: return).toString(), percentage = percentage)
        }
        app.sync.save(book.document, position, locator.toJSON().toString())
    }

    // -----------------------------------------------------------------------
    // The text selection: highlight, note, "Set as last position", copy

    private val selectionCallback = object : ActionMode.Callback {
        override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
            menu.add(Menu.NONE, HIGHLIGHT_ITEM, 0, R.string.highlight)
            menu.add(Menu.NONE, NOTE_ITEM, 1, R.string.add_note)
            menu.add(Menu.NONE, MARK_ITEM, 2, R.string.set_last_position)
            menu.add(Menu.NONE, COPY_ITEM, 3, android.R.string.copy)
            return true
        }

        override fun onPrepareActionMode(mode: ActionMode, menu: Menu) = false

        override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
            when (item.itemId) {
                HIGHLIGHT_ITEM, NOTE_ITEM -> lifecycleScope.launch {
                    highlightSelection(withNote = item.itemId == NOTE_ITEM)
                    mode.finish()
                }
                MARK_ITEM -> lifecycleScope.launch {
                    markSelection()
                    mode.finish()
                }
                COPY_ITEM -> lifecycleScope.launch {
                    copySelection()
                    mode.finish()
                }
                else -> return false
            }
            return true
        }

        override fun onDestroyActionMode(mode: ActionMode) {}
    }

    private suspend fun markSelection() {
        val epub = navigator as? EpubNavigatorFragment ?: return
        val locator = epub.currentLocator.value
        val index = publication?.readingOrder?.indexOfFirstWithHref(locator.href) ?: return
        val point = js("OPDShelf.selection($index)")?.let { decodePoint(it) } ?: return
        epub.clearSelection()
        val position = Position(
            progress = point.progress,
            percentage = (locator.locations.totalProgression ?: 0.0).coerceIn(0.0, 1.0),
            context = point.context,
            manual = true,
        )
        app.sync.save(book.document, position, locator.toJSON().toString(), now = true)
        // Kept until reading moves on, like KOReader does
        baseline = epub.currentLocator.value
        dismissHint()
        Snackbar.make(root, R.string.last_position_set, Snackbar.LENGTH_LONG).show()
    }

    private suspend fun copySelection() {
        val epub = navigator as? EpubNavigatorFragment ?: return
        val text = epub.currentSelection()?.locator?.text?.highlight ?: return
        epub.clearSelection()
        getSystemService(ClipboardManager::class.java)
            .setPrimaryClip(ClipData.newPlainText(book.title, text))
    }

    // -----------------------------------------------------------------------
    // JavaScript inside the EPUB page

    private suspend fun js(expression: String): String? {
        val epub = navigator as? EpubNavigatorFragment ?: return null
        val setup = scripts ?: (assets.open("reader/xpointer.js").reader().use { it.readText() } + "\n" +
            assets.open("reader/bridge.js").reader().use { it.readText() }).also { scripts = it }
        epub.evaluateJavascript("if (!window.OPDShelf) {\n$setup\n}")
        return epub.evaluateJavascript(expression)?.takeIf { it != "null" && it != "undefined" }
    }

    private fun decodePoint(text: String): Point? = runCatching { json.decodeFromString<Point>(text) }.getOrNull()

    private fun quote(s: String) = json.encodeToString(s)

    private fun contextJson(context: PositionContext?) =
        context?.let { json.encodeToString(it) } ?: "null"

    // -----------------------------------------------------------------------
    // Reading settings and bars

    private val scroll get() = ui.prefs.scroll

    private fun loadPrefs(): ReaderPrefs =
        prefs.getString("prefs", null)?.let { runCatching { json.decodeFromString<ReaderPrefs>(it) }.getOrNull() }
            ?: ReaderPrefs(
                fontSize = prefs.getFloat("fontSize", 1f).toDouble(),
                theme = prefs.getInt("theme", 0).coerceIn(0, 2),
                scroll = prefs.getBoolean("scroll", false),
            )

    private fun epubPreferences(p: ReaderPrefs = ui.prefs): EpubPreferences {
        val custom = p.font != 0 || p.spacing != 1 || p.justify
        return EpubPreferences(
            fontSize = p.fontSize,
            scroll = p.scroll,
            theme = when (p.theme) {
                1 -> Theme.SEPIA
                2 -> Theme.DARK
                else -> Theme.LIGHT
            },
            fontFamily = when (p.font) {
                1 -> FontFamily.SERIF
                2 -> FontFamily.SANS_SERIF
                else -> null
            },
            lineHeight = when (p.spacing) {
                0 -> 1.25
                2 -> 1.8
                else -> null
            },
            pageMargins = when (p.margins) {
                0 -> 0.5
                2 -> 1.6
                else -> null
            },
            textAlign = if (p.justify) org.readium.r2.navigator.preferences.TextAlign.JUSTIFY else null,
            publisherStyles = if (custom) false else null,
        )
    }

    private fun dismissHint() {
        app.settings.markHintShown = true
        ui.showHint = false
    }

    private val actions = object : ReaderActions {
        override fun close() = finish()

        override fun goToProgression(progression: Double) {
            val pub = publication ?: return
            val nav = navigator ?: return
            lifecycleScope.launch { pub.locateProgression(progression)?.let { nav.go(it) } }
        }

        override fun goTo(item: TocItem) {
            val nav = navigator ?: return
            lifecycleScope.launch { nav.go(item.link) }
        }

        override fun changePrefs(prefs: ReaderPrefs) {
            ui.prefs = prefs
            this@ReaderActivity.prefs.edit().putString("prefs", json.encodeToString(prefs)).apply()
            (navigator as? EpubNavigatorFragment)?.submitPreferences(epubPreferences(prefs))
            showBars(ui.visible)
            applyScreenOn()
            applyBrightness()
        }

        override fun dismissHint() = this@ReaderActivity.dismissHint()

        override fun toggleBookmark() {
            val locator = navigator?.currentLocator?.value ?: return
            val current = ui.currentBookmark
            val list = if (current != null) {
                ui.bookmarks - current
            } else {
                ui.bookmarks + Bookmark(
                    locator = locator.toJSON().toString(),
                    chapter = ui.chapter,
                    progression = (locator.locations.totalProgression ?: 0.0).coerceIn(0.0, 1.0),
                    text = locator.text.highlight?.take(120),
                )
            }
            ui.bookmarks = list
            app.store.setBookmarks(book.id, list)
            ui.currentBookmark = list.firstOrNull { onPage(it, locator) }
            if (current == null) Snackbar.make(root, R.string.bookmark_added, Snackbar.LENGTH_SHORT).show()
            else Snackbar.make(root, R.string.bookmark_removed, Snackbar.LENGTH_LONG).setAction(R.string.undo) { restore(current) }.show()
        }

        override fun goTo(bookmark: Bookmark) {
            val locator = runCatching { Locator.fromJSON(JSONObject(bookmark.locator)) }.getOrNull() ?: return
            navigator?.go(locator)
        }

        override fun restore(bookmark: Bookmark) {
            if (bookmark in ui.bookmarks) return
            ui.bookmarks = ui.bookmarks + bookmark
            app.store.setBookmarks(book.id, ui.bookmarks)
            ui.removedBookmark = null
            ui.currentBookmark = navigator?.currentLocator?.value?.let { l -> ui.bookmarks.firstOrNull { onPage(it, l) } }
        }

        override fun remove(bookmark: Bookmark) {
            ui.removedBookmark = bookmark
            ui.bookmarks = ui.bookmarks - bookmark
            app.store.setBookmarks(book.id, ui.bookmarks)
            ui.currentBookmark = navigator?.currentLocator?.value?.let { l -> ui.bookmarks.firstOrNull { onPage(it, l) } }
        }

        override fun search(query: String) {
            val pub = publication ?: return
            searchJob?.cancel()
            ui.searchResults = emptyList()
            ui.searching = true
            ui.searched = false
            searchJob = lifecycleScope.launch {
                try {
                    val iterator = pub.search(query) ?: return@launch
                    val hits = mutableListOf<SearchHit>()
                    while (hits.size < SEARCH_LIMIT) {
                        val page = iterator.next().getOrNull() ?: break
                        page.locators.forEach { l ->
                            hits += SearchHit(
                                locator = l,
                                chapter = l.title ?: chapterOf(l.href.toString()),
                                before = l.text.before.orEmpty().replace(Regex("\\s+"), " "),
                                match = l.text.highlight.orEmpty(),
                                after = l.text.after.orEmpty().replace(Regex("\\s+"), " "),
                            )
                        }
                        ui.searchResults = hits.take(SEARCH_LIMIT)
                    }
                    iterator.close()
                } finally {
                    ui.searching = false
                    ui.searched = true
                }
            }
        }

        override fun goTo(hit: SearchHit) {
            navigator?.go(hit.locator)
        }

        override fun goTo(annotation: io.github.danielpereod.opdshelf.data.Annotation) = goToAnnotation(annotation)

        override fun save(annotation: io.github.danielpereod.opdshelf.data.Annotation) {
            prefs.edit().putString("highlight_color", annotation.color).apply()
            app.sync.annotate(book.document, annotation)
        }

        override fun delete(annotation: io.github.danielpereod.opdshelf.data.Annotation) {
            app.sync.annotate(book.document, annotation.copy(deleted = true))
            Snackbar.make(root, R.string.highlight_deleted, Snackbar.LENGTH_LONG)
                .setAction(R.string.undo) { app.sync.annotate(book.document, annotation.copy(deleted = false)) }
                .show()
        }

        override fun shareAnnotations() {
            val text = io.github.danielpereod.opdshelf.data.annotationsMarkdown(
                book.title, book.authors.joinToString(", "), ui.annotations, getString(R.string.no_chapter),
            )
            startActivity(
                android.content.Intent.createChooser(
                    android.content.Intent(android.content.Intent.ACTION_SEND)
                        .setType("text/plain")
                        .putExtra(android.content.Intent.EXTRA_SUBJECT, book.title)
                        .putExtra(android.content.Intent.EXTRA_TEXT, text),
                    getString(R.string.share_highlights),
                )
            )
        }
    }

    private var searchJob: kotlinx.coroutines.Job? = null

    /** Whether a bookmark falls on the page of [locator]. */
    private fun onPage(mark: Bookmark, locator: Locator): Boolean {
        val saved = runCatching { Locator.fromJSON(JSONObject(mark.locator)) }.getOrNull() ?: return false
        if (saved.href != locator.href) return false
        saved.locations.position?.let { p -> locator.locations.position?.let { return it == p } }
        val a = saved.locations.progression ?: return false
        val b = locator.locations.progression ?: return false
        return kotlin.math.abs(a - b) < 0.01
    }

    private fun applyBrightness() {
        val b = ui.prefs.brightness
        window.attributes = window.attributes.apply {
            screenBrightness = if (b < 0) android.view.WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE else b.coerceIn(0.02f, 1f)
        }
    }

    private fun applyScreenOn() {
        if (ui.prefs.keepScreenOn) window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        else window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    // Volume keys turn the page, while the bars are hidden and the setting is on
    private fun pageKey(keyCode: Int): Int = when {
        !ui.prefs.volumeKeys || ui.visible || ui.sheet != null -> 0
        keyCode == KeyEvent.KEYCODE_VOLUME_DOWN -> 1
        keyCode == KeyEvent.KEYCODE_VOLUME_UP -> -1
        else -> 0
    }

    override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
        val nav = navigator as? OverflowableNavigator
        when (pageKey(keyCode)) {
            1 -> if (nav != null) { nav.goForward(); return true }
            -1 -> if (nav != null) { nav.goBackward(); return true }
        }
        return super.onKeyDown(keyCode, event)
    }

    override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean =
        if (pageKey(keyCode) != 0 && navigator is OverflowableNavigator) true else super.onKeyUp(keyCode, event)

    private fun flatten(links: List<Link>, depth: Int = 0): List<TocItem> = links.flatMap { link ->
        listOf(TocItem(link.title?.trim().orEmpty().ifEmpty { "—" }, depth, link.href.toString(), link)) +
            flatten(link.children, depth + 1)
    }

    /** The table of contents' name for a chapter, when the locator has none. */
    private fun chapterOf(href: String): String? =
        ui.toc.lastOrNull { it.href.substringBefore('#') == href.substringBefore('#') }?.title

    private fun toggleBars() = showBars(!ui.visible)

    private fun showBars(show: Boolean) {
        ui.visible = show
        if (!show) ui.sheet = null
        val controller = WindowInsetsControllerCompat(window, window.decorView)
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        val night = ui.prefs.theme == 2
        controller.isAppearanceLightStatusBars = !night
        controller.isAppearanceLightNavigationBars = !night
        if (show) controller.show(WindowInsetsCompat.Type.systemBars())
        else controller.hide(WindowInsetsCompat.Type.systemBars())
    }
}

/** Spine index (from 0) of a KOReader xpointer, or -1. */
fun spineIndex(xpointer: String): Int {
    val match = Regex("""^/body/DocFragment(?:\[(\d+)])?(?:/|$)""").find(xpointer) ?: return -1
    return (match.groupValues[1].toIntOrNull() ?: 1) - 1
}

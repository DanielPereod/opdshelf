package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material.icons.automirrored.outlined.Sort
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.DoneAll
import androidx.compose.material.icons.outlined.RemoveDone
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Deselect
import androidx.compose.material.icons.outlined.SelectAll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.Icon
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.focus.focusRequester
import androidx.compose.foundation.layout.Box
import androidx.compose.material.icons.automirrored.outlined.MenuBook
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Info
import androidx.compose.runtime.remember
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.LibraryAdd
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.ImageLoader
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.LocalBook
import java.io.File

private enum class Sort(val label: Int) { Recent(R.string.sort_recent), Title(R.string.sort_title), Progress(R.string.sort_progress) }

private enum class Filter(val label: Int, val test: (LocalBook) -> Boolean) {
    All(R.string.filter_all, { true }),
    Reading(R.string.filter_reading, { it.percentage > 0 && !it.finished }),
    Unread(R.string.filter_unread, { it.percentage <= 0 && !it.finished }),
    Finished(R.string.filter_finished, { it.finished }),
}

/** Home: the book to continue, then everything downloaded to this device. */
@Composable
fun HomeScreen(
    library: LibraryViewModel,
    openBook: (LocalBook) -> Unit,
    showDetail: (LocalBook) -> Unit,
    goExplore: () -> Unit,
    modifier: Modifier,
    openStats: () -> Unit = {},
) {
    val books by library.books.collectAsState()
    val stats by library.stats.collectAsState()
    // The day's reading may have changed on any device since last time
    LaunchedEffect(Unit) { library.loadStats() }
    val status by library.syncStatus.collectAsState()
    val loader = rememberImageLoader()
    val context = LocalContext.current
    var sort by rememberSaveable { mutableStateOf(Sort.Recent) }
    var filter by rememberSaveable { mutableStateOf(Filter.All) }
    var searching by rememberSaveable { mutableStateOf(false) }
    var query by rememberSaveable { mutableStateOf("") }
    var removing by remember { mutableStateOf<LocalBook?>(null) }
    // Ids picked by a long press; empty means not selecting
    var picked by rememberSaveable { mutableStateOf(listOf<String>()) }
    var removingPicked by remember { mutableStateOf(false) }
    val selecting = picked.isNotEmpty()
    val wide = isWide
    // Books removed elsewhere drop out of the selection
    LaunchedEffect(books) { picked = picked.filter { id -> books.any { it.id == id } } }
    androidx.activity.compose.BackHandler(selecting) { picked = emptyList() }
    fun toggle(book: LocalBook) { picked = if (book.id in picked) picked - book.id else picked + book.id }

    // The books shown, as filtered, searched and sorted
    val shown = remember(books, sort, filter, query) {
        val words = query.trim().lowercase().split(Regex("\\s+")).filter { it.isNotEmpty() }
        books.filter(filter.test).filter { book ->
            val haystack = (book.title + " " + book.authors.joinToString(" ")).lowercase()
            words.all { it in haystack }
        }.let { list ->
            when (sort) {
                Sort.Recent -> list.sortedByDescending { maxOf(it.lastOpenedAt, it.addedAt) }
                Sort.Title -> list.sortedBy { it.title.lowercase() }
                Sort.Progress -> list.sortedByDescending { it.percentage }
            }
        }
    }

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        if (selecting) {
            val allFinished = books.filter { it.id in picked }.all { it.finished }
            Header(
                pluralStringResource(R.plurals.selected_count, picked.size, picked.size),
                onBack = { picked = emptyList() },
                backIcon = Icons.Outlined.Close, backLabel = stringResource(R.string.clear_selection), titleMaxLines = 1,
                actions = {
                    if (shown.isNotEmpty()) {
                        val all = allPicked(picked, shown.map { it.id })
                        IconButton(onClick = { picked = toggleAll(picked, shown.map { it.id }) }) {
                            Icon(
                                if (all) Icons.Outlined.Deselect else Icons.Outlined.SelectAll,
                                stringResource(if (all) R.string.deselect_all else R.string.select_all), tint = shelf.fg,
                            )
                        }
                    }
                    IconButton(onClick = { library.setFinished(picked, !allFinished); picked = emptyList() }) {
                        Icon(
                            if (allFinished) Icons.Outlined.RemoveDone else Icons.Outlined.DoneAll,
                            stringResource(if (allFinished) R.string.mark_unfinished else R.string.mark_selected_finished), tint = shelf.fg,
                        )
                    }
                    IconButton(onClick = { removingPicked = true }) {
                        Icon(Icons.Outlined.Delete, stringResource(R.string.remove_from_device), tint = shelf.danger)
                    }
                },
            )
        } else Header(
            stringResource(R.string.tab_home),
            // The rail already shows the logo on wide screens
            leading = if (wide) null else ({ Logo(24.dp) }),
            actions = {
                if (books.isNotEmpty()) {
                    IconButton(onClick = { searching = !searching; if (!searching) query = "" }) {
                        Icon(
                            if (searching) Icons.Outlined.Close else Icons.Outlined.Search,
                            stringResource(if (searching) R.string.close_search else R.string.search_my_books), tint = shelf.fg,
                        )
                    }
                    SyncButton(status) { library.syncNow() }
                }
            },
        )
        if (books.isEmpty()) {
            EmptyState(
                icon = Icons.Outlined.LibraryAdd,
                title = stringResource(R.string.home_empty_title),
                text = stringResource(R.string.home_empty_text),
                action = stringResource(R.string.explore_library),
                onAction = goExplore,
            )
            return
        }
        if (searching) {
            val focus = remember { androidx.compose.ui.focus.FocusRequester() }
            LaunchedEffect(Unit) { focus.requestFocus() }
            Column(Modifier.background(shelf.pane)) {
                SearchBox(
                    query, { query = it }, stringResource(R.string.search_my_books), onSearch = {},
                    modifier = Modifier.fillMaxWidth().padding(horizontal = edgePadding, vertical = Space.m).focusRequester(focus),
                )
                HorizontalDivider(color = shelf.line)
            }
        }
        val current = remember(books) {
            books.filter { it.lastOpenedAt > 0 && !it.finished }.maxByOrNull { it.lastOpenedAt }
                ?: books.filter { it.percentage > 0 && !it.finished }.maxByOrNull { it.percentage }
        }
        val counts = remember(books) { Filter.entries.associateWith { f -> books.count(f.test) } }
        val used = remember(books) { formatSize(context.app.store.usedBytes()) }
        // Pulling down sends what is waiting and reads the server's positions and marks again
        val pull = androidx.compose.material3.pulltorefresh.rememberPullToRefreshState()
        androidx.compose.material3.pulltorefresh.PullToRefreshBox(
            isRefreshing = status.syncing,
            onRefresh = { library.syncNow() },
            state = pull,
            modifier = Modifier.fillMaxSize(),
            indicator = {
                androidx.compose.material3.pulltorefresh.PullToRefreshDefaults.Indicator(
                    state = pull, isRefreshing = status.syncing, modifier = Modifier.align(Alignment.TopCenter),
                    containerColor = shelf.pane, color = shelf.fg,
                )
            },
        ) {
        LazyVerticalGrid(
            columns = GridCells.Adaptive(gridCellWidth),
            contentPadding = PaddingValues(horizontal = edgePadding, vertical = Space.l),
            horizontalArrangement = Arrangement.spacedBy(if (wide) Space.l else Space.m),
            verticalArrangement = Arrangement.spacedBy(Space.xl),
            modifier = Modifier.fillMaxSize(),
        ) {
            // Syncing is silent while it works: a line only when positions are waiting to be sent
            if (status.pending > 0) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    SyncLine(status, onSync = { library.syncNow() })
                }
            }
            // The card and the strip span the grid as the rows of books under them do, so
            // every block ends on the same right margin
            if (current != null && query.isBlank() && filter == Filter.All && !selecting) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    ContinueCard(current, loader, onRead = { openBook(current) }, onDetails = { showDetail(current) })
                }
            }
            // Today's reading and the streak, leading to the statistics: there from the
            // server's first answer, even with no reading yet, so they can always be reached
            if (query.isBlank() && filter == Filter.All && !selecting && stats.stats != null) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    StatsStrip(stats, onOpen = openStats)
                }
            }
            item(span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.padding(top = Space.s)) {
                    SectionTitle(
                        listOfNotNull(
                            pluralStringResource(R.plurals.books_on_device, books.size, books.size),
                            used,
                        ).joinToString(" · "),
                        trailing = { SortMenu(sort) { sort = it } },
                    )
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
                        Filter.entries.forEach { f ->
                            val n = counts[f] ?: 0
                            if (f == Filter.All || f == Filter.Finished || n > 0 || filter == f) {
                                Pill(if (f == Filter.All) stringResource(f.label) else "${stringResource(f.label)} $n", filter == f) { filter = f }
                            }
                        }
                    }
                }
            }
            if (shown.isEmpty()) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    Column(Modifier.fillMaxWidth().padding(vertical = Space.xxl), horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(stringResource(R.string.no_books_filter), color = shelf.fg, fontSize = TextSize.subtitle, fontWeight = FontWeight.SemiBold)
                        Spacer(Modifier.height(Space.xs))
                        Text(stringResource(R.string.no_books_filter_text), color = shelf.muted, fontSize = TextSize.bodySmall)
                    }
                }
            }
            items(shown, key = { it.id }) { book ->
                BookTile(
                    title = book.title, authors = book.authors,
                    cover = book.coverFile?.let { File(context.app.store.booksDir, it) },
                    format = book.format ?: book.fileName.substringAfterLast('.', ""),
                    loader = loader, progress = book.percentage, finished = book.finished,
                    onClick = { if (selecting) toggle(book) else openBook(book) },
                    onLongClick = { toggle(book) },
                    selected = if (selecting) book.id in picked else null,
                    longClickLabel = stringResource(R.string.select),
                    menu = listOf(
                        MenuAction(stringResource(R.string.read), Icons.AutoMirrored.Outlined.MenuBook) { openBook(book) },
                        MenuAction(stringResource(R.string.details), Icons.Outlined.Info) { showDetail(book) },
                        if (book.finished) MenuAction(stringResource(R.string.mark_unfinished), Icons.Outlined.RemoveDone) { library.setFinished(book, false) }
                        else MenuAction(stringResource(R.string.mark_finished), Icons.Outlined.DoneAll) { library.setFinished(book, true) },
                        MenuAction(stringResource(R.string.remove_from_device), Icons.Outlined.Delete, destructive = true) { removing = book },
                    ),
                )
            }
        }
        }
    }


    if (removingPicked) {
        ConfirmDialog(
            title = pluralStringResource(R.plurals.remove_selected, picked.size, picked.size),
            text = stringResource(R.string.remove_selected_text),
            confirm = stringResource(R.string.remove),
            onConfirm = {
                removingPicked = false
                library.delete(picked)
                picked = emptyList()
            },
            onDismiss = { removingPicked = false },
        )
    }

    removing?.let { book ->
        ConfirmDialog(
            title = stringResource(R.string.remove_title),
            text = stringResource(R.string.remove_text, book.title),
            confirm = stringResource(R.string.remove),
            onConfirm = {
                removing = null
                library.delete(book)
            },
            onDismiss = { removing = null },
        )
    }
}

/**
 * How the card to continue is laid out in [widthDp] of content (inside its padding): a
 * phone's small cover with the button under the text; from 480 dp a tablet's, larger
 * cover, the description filling the width and the button at the bottom right, beside
 * how far and where it was read, with more of the description on a wider card.
 */
data class ContinueLayout(val coverWidthDp: Int, val coverHeightDp: Int, val summaryLines: Int, val large: Boolean)

fun continueLayout(widthDp: Float): ContinueLayout = when {
    widthDp >= 840f -> ContinueLayout(coverWidthDp = 160, coverHeightDp = 240, summaryLines = 4, large = true)
    widthDp >= 480f -> ContinueLayout(coverWidthDp = 140, coverHeightDp = 210, summaryLines = 3, large = true)
    else -> ContinueLayout(coverWidthDp = 88, coverHeightDp = 132, summaryLines = 0, large = false)
}

@Composable
private fun ContinueCard(book: LocalBook, loader: ImageLoader, onRead: () -> Unit, onDetails: () -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val position = remember(book) { context.app.store.position(book.document)?.position }
    val settings = context.app.settings
    androidx.compose.foundation.layout.BoxWithConstraints(
        modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(shelf.pane)
            .border(1.dp, shelf.line, RoundedCornerShape(12.dp))
            .clickable(onClickLabel = stringResource(R.string.details), onClick = onDetails)
            .padding(Space.l),
    ) {
        val layout = continueLayout(maxWidth.value)
        val large = layout.large
        val coverHeight = layout.coverHeightDp.dp
        val where = position?.takeIf { it.timestamp > 0 && it.device.isNotEmpty() }?.let {
            // This device by the name it has now, not the model's code it was sent with once
            val device = if (it.deviceId == settings.deviceId) settings.deviceName else it.device
            stringResource(R.string.read_on, device, relativeTime(it.timestamp * 1000))
        }
        val status = listOfNotNull(percent(book.percentage), where).joinToString(" · ")
        val button = stringResource(if (book.percentage > 0) R.string.continue_reading else R.string.read)
        val summary = book.summary?.trim()?.takeIf { it.isNotEmpty() && layout.summaryLines > 0 }
        // With a description the text fills the cover's height, the progress on its bottom
        // edge; without one the text is centred beside the cover rather than leaving a band
        val pinned = large && summary != null
        Row(verticalAlignment = if (large && !pinned) Alignment.CenterVertically else Alignment.Top) {
            Cover(
                book.coverFile?.let { File(context.app.store.booksDir, it) }, book.title,
                book.format, loader, Modifier.size(width = layout.coverWidthDp.dp, height = coverHeight),
                large = large,
            )
            Spacer(Modifier.width(if (large) Space.xl else Space.l))
            Column(Modifier.weight(1f).then(if (pinned) Modifier.heightIn(min = coverHeight) else Modifier)) {
                Text(
                    book.title, color = shelf.fg, fontSize = if (large) TextSize.headline else TextSize.title, fontWeight = FontWeight.Bold,
                    maxLines = if (large) 2 else 3, overflow = TextOverflow.Ellipsis, lineHeight = if (large) 28.sp else 26.sp,
                )
                if (book.authors.isNotEmpty()) {
                    Spacer(Modifier.height(Space.xs))
                    Text(
                        book.authors.joinToString(", "), color = shelf.muted, fontSize = if (large) TextSize.body else TextSize.bodySmall,
                        lineHeight = 20.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    )
                }
                if (summary != null) {
                    Spacer(Modifier.height(Space.m))
                    Text(
                        summary, color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp,
                        maxLines = layout.summaryLines, overflow = TextOverflow.Ellipsis,
                    )
                }
                if (large) {
                    if (pinned) Spacer(Modifier.weight(1f))
                    Spacer(Modifier.height(Space.l))
                    ReadBar(book.percentage.toFloat(), Modifier.fillMaxWidth())
                    Spacer(Modifier.height(Space.m))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            status, color = shelf.muted, fontSize = TextSize.label, lineHeight = 18.sp, maxLines = 2,
                            overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                        )
                        Spacer(Modifier.width(Space.l))
                        PrimaryButton(button, onRead, height = 48.dp)
                    }
                } else {
                    Spacer(Modifier.height(Space.m))
                    ReadBar(book.percentage.toFloat(), Modifier.fillMaxWidth())
                    Spacer(Modifier.height(Space.s))
                    Text(status, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 2)
                    Spacer(Modifier.height(Space.l))
                    PrimaryButton(button, onRead, height = 48.dp)
                }
            }
        }
    }
}

/** "Sort ▾" with a menu of the orders. */
@Composable
private fun SortMenu(sort: Sort, onChange: (Sort) -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        androidx.compose.material3.TextButton(onClick = { open = true }, modifier = Modifier.heightIn(min = 48.dp)) {
            Icon(Icons.AutoMirrored.Outlined.Sort, null, tint = shelf.fg, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(Space.s))
            Text(stringResource(sort.label), color = shelf.fg, fontSize = TextSize.label, fontWeight = FontWeight.SemiBold)
        }
        androidx.compose.material3.DropdownMenu(open, { open = false }, containerColor = shelf.pane) {
            Sort.entries.forEach { s ->
                androidx.compose.material3.DropdownMenuItem(
                    text = { Text(stringResource(s.label), color = shelf.fg, fontWeight = if (s == sort) FontWeight.Bold else FontWeight.Normal) },
                    trailingIcon = { if (s == sort) Icon(Icons.Outlined.Check, null, tint = shelf.fg) },
                    onClick = { open = false; onChange(s) },
                )
            }
        }
    }
}

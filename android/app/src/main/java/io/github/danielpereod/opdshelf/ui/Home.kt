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
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.Icon
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.focus.focusRequester
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.widthIn
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
) {
    val books by library.books.collectAsState()
    val status by library.syncStatus.collectAsState()
    val loader = rememberImageLoader()
    val context = LocalContext.current
    var sort by rememberSaveable { mutableStateOf(Sort.Recent) }
    var filter by rememberSaveable { mutableStateOf(Filter.All) }
    var searching by rememberSaveable { mutableStateOf(false) }
    var query by rememberSaveable { mutableStateOf("") }
    var removing by remember { mutableStateOf<LocalBook?>(null) }
    val wide = isWide

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(
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
        val counts = remember(books) { Filter.entries.associateWith { f -> books.count(f.test) } }
        val used = remember(books) { formatSize(context.app.store.usedBytes()) }
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
            if (current != null && query.isBlank() && filter == Filter.All) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    Box(Modifier.fillMaxWidth()) {
                        ContinueCard(
                            current, loader, onRead = { openBook(current) }, onDetails = { showDetail(current) },
                            modifier = Modifier.widthIn(max = 720.dp),
                        )
                    }
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
                            if (f == Filter.All || n > 0 || filter == f) {
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
                    onClick = { openBook(book) }, onLongClick = { showDetail(book) },
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

@Composable
private fun ContinueCard(book: LocalBook, loader: ImageLoader, onRead: () -> Unit, onDetails: () -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val position = remember(book) { context.app.store.position(book.document)?.position }
    Column(
        modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(shelf.pane)
            .border(1.dp, shelf.line, RoundedCornerShape(12.dp))
            .clickable(onClickLabel = stringResource(R.string.details), onClick = onDetails)
            .padding(Space.l),
    ) {
        Row {
            Cover(
                book.coverFile?.let { File(context.app.store.booksDir, it) }, book.title,
                book.format, loader, Modifier.size(width = 88.dp, height = 132.dp),
            )
            Spacer(Modifier.width(Space.l))
            Column(Modifier.weight(1f)) {
                Text(book.title, color = shelf.fg, fontSize = TextSize.title, fontWeight = FontWeight.Bold, maxLines = 3, overflow = TextOverflow.Ellipsis, lineHeight = 26.sp)
                if (book.authors.isNotEmpty()) {
                    Spacer(Modifier.height(Space.xs))
                    Text(book.authors.joinToString(", "), color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Spacer(Modifier.height(Space.m))
                ReadBar(book.percentage.toFloat(), Modifier.fillMaxWidth())
                Spacer(Modifier.height(Space.s))
                val where = position?.takeIf { it.timestamp > 0 && it.device.isNotEmpty() }?.let {
                    stringResource(R.string.read_on, it.device, relativeTime(it.timestamp * 1000))
                }
                Text(
                    listOfNotNull(percent(book.percentage), where).joinToString(" · "),
                    color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 2,
                )
                Spacer(Modifier.height(Space.l))
                PrimaryButton(
                    stringResource(if (book.percentage > 0) R.string.continue_reading else R.string.read),
                    onRead, height = 48.dp,
                )
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

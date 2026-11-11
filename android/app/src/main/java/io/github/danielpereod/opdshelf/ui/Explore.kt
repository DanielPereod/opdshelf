package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material.icons.automirrored.outlined.LibraryBooks
import androidx.compose.material.icons.automirrored.outlined.MenuBook
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.CollectionsBookmark
import androidx.compose.material.icons.outlined.DownloadDone
import androidx.compose.material.icons.outlined.FolderOff
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.PersonOff
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ViewList
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.outlined.SearchOff
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.ImageLoader
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.data.Entry
import io.github.danielpereod.opdshelf.data.LocalBook

/** The web's list/grid switch: two icons in a hairline box. */
@Composable
private fun ViewSwitch(grid: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.padding(end = Space.s).clip(RoundedCornerShape(8.dp)).border(1.dp, shelf.lineStrong, RoundedCornerShape(8.dp)).padding(2.dp)) {
        SwitchOption(Icons.AutoMirrored.Outlined.ViewList, stringResource(R.string.view_list), !grid) { onChange(false) }
        SwitchOption(Icons.Outlined.GridView, stringResource(R.string.view_grid), grid) { onChange(true) }
    }
}

@Composable
private fun SwitchOption(icon: ImageVector, label: String, selected: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.size(width = 48.dp, height = 40.dp).clip(RoundedCornerShape(6.dp))
            .background(if (selected) shelf.soft else shelf.pane)
            .selectable(selected = selected, role = Role.Tab, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, label, tint = if (selected) shelf.fg else shelf.muted, modifier = Modifier.size(20.dp)) }
}

/** The server's catalog: sections, search, and books to open or download. */
@Composable
fun ExploreScreen(library: LibraryViewModel, openBook: (LocalBook) -> Unit, modifier: Modifier) {
    val stack by library.stack.collectAsState()
    val section by library.section.collectAsState()
    val downloads by library.downloads.collectAsState()
    val books by library.books.collectAsState()
    val loader = rememberImageLoader()
    val state = stack.lastOrNull()
    var query by rememberSaveable { mutableStateOf("") }
    var grid by rememberSaveable { mutableStateOf(true) }
    val focus = LocalFocusManager.current
    // Search and sections slide away while scrolling down and come back on the way up,
    // so short screens (phones on their side) keep room for the books
    var filtersShown by remember { mutableStateOf(true) }
    val hideOnScroll = remember {
        object : androidx.compose.ui.input.nestedscroll.NestedScrollConnection {
            override fun onPreScroll(available: androidx.compose.ui.geometry.Offset, source: androidx.compose.ui.input.nestedscroll.NestedScrollSource): androidx.compose.ui.geometry.Offset {
                if (available.y < -8f) filtersShown = false
                if (available.y > 8f) filtersShown = true
                return androidx.compose.ui.geometry.Offset.Zero
            }
        }
    }

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        val entries = state?.feed?.entries.orEmpty()
        val hasBooks = entries.any { it.acquisition != null }
        val atRoot = stack.size <= 1
        Header(
            title = state?.label ?: if (atRoot) stringResource(R.string.tab_explore) else state?.feed?.title.orEmpty(),
            count = entries.size.takeIf { hasBooks && state?.feed?.nextUrl == null },
            onBack = if (!atRoot) ({ library.back() }) else null,
            actions = { if (hasBooks) ViewSwitch(grid) { grid = it } },
        )
        androidx.compose.animation.AnimatedVisibility(filtersShown || query.isNotEmpty()) {
        Column(Modifier.background(shelf.pane)) {
            SearchBox(
                value = query, onChange = { query = it }, placeholder = stringResource(R.string.search_hint),
                onSearch = {
                    if (query.isNotBlank()) {
                        focus.clearFocus()
                        library.search(query.trim())
                    }
                },
                modifier = Modifier.fillMaxWidth().padding(start = edgePadding, end = edgePadding, top = Space.m),
            )
            if (atRoot) {
                Row(
                    Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = edgePadding, vertical = Space.xs),
                    horizontalArrangement = Arrangement.spacedBy(Space.s),
                ) {
                    Section.entries.forEach { s -> Pill(stringResource(s.label), section == s) { library.selectSection(s) } }
                }
            } else {
                Spacer(Modifier.size(Space.m))
            }
            HorizontalDivider(color = shelf.line)
        }
        }
        when {
            state == null -> {}
            state.loading -> Box(Modifier.fillMaxSize(), Alignment.Center) { Spinner() }
            state.error != null -> EmptyState(
                icon = Icons.Outlined.CloudOff,
                title = stringResource(R.string.cannot_load_title),
                text = state.error,
                action = stringResource(R.string.retry),
                onAction = { library.retry() },
            )
            entries.isEmpty() -> {
                // Each section says why it is empty
                val (icon, title, text) = when {
                    state.label != null -> Triple(Icons.Outlined.SearchOff, R.string.no_results, R.string.no_results_text)
                    !atRoot -> Triple(Icons.Outlined.FolderOff, R.string.empty_feed, R.string.empty_feed_text)
                    section == Section.Authors -> Triple(Icons.Outlined.PersonOff, R.string.empty_authors, R.string.empty_authors_text)
                    section == Section.Series -> Triple(Icons.Outlined.CollectionsBookmark, R.string.empty_series, R.string.empty_series_text)
                    section == Section.Folders -> Triple(Icons.Outlined.FolderOff, R.string.empty_folders, R.string.empty_folders_text)
                    else -> Triple(Icons.AutoMirrored.Outlined.LibraryBooks, R.string.empty_library, R.string.empty_library_text)
                }
                EmptyState(icon = icon, title = stringResource(title), text = stringResource(text))
            }
            else -> {
                val list = rememberLazyGridState()
                val nearEnd by remember {
                    derivedStateOf { (list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0) >= entries.size - 6 }
                }
                LaunchedEffect(nearEnd, entries.size) { if (nearEnd) library.loadMore() }
                val useGrid = grid && hasBooks
                LazyVerticalGrid(
                    state = list,
                    columns = if (useGrid) GridCells.Adaptive(gridCellWidth) else GridCells.Fixed(1),
                    contentPadding = if (useGrid) PaddingValues(horizontal = edgePadding, vertical = Space.l) else PaddingValues(0.dp),
                    horizontalArrangement = Arrangement.spacedBy(if (useGrid) (if (isWide) Space.l else Space.m) else 0.dp),
                    verticalArrangement = Arrangement.spacedBy(if (useGrid) Space.xl else 0.dp),
                    modifier = Modifier.fillMaxSize().nestedScroll(hideOnScroll),
                ) {
                    items(
                        entries, key = { it.id + it.feedUrl },
                        span = { if (useGrid && it.feedUrl != null) GridItemSpan(maxLineSpan) else GridItemSpan(1) },
                    ) { entry ->
                        val local = books.firstOrNull { it.id == entry.id }
                        val progress = downloads[entry.id]
                        val feed = entry.feedUrl
                        when {
                            feed != null -> FolderRow(entry, section) { library.open(feed) }
                            useGrid -> BookTile(
                                title = entry.title, authors = entry.authors,
                                cover = entry.coverUrl ?: entry.thumbnailUrl, format = entry.acquisition?.format,
                                loader = loader, onClick = { library.showDetail(entry, entry.id) },
                                progress = local?.percentage?.takeIf { it > 0 },
                                downloaded = local != null, downloading = progress,
                                menu = listOfNotNull(
                                    when {
                                        local != null -> MenuAction(stringResource(R.string.read), Icons.AutoMirrored.Outlined.MenuBook) { openBook(local) }
                                        progress != null -> MenuAction(stringResource(R.string.cancel_download), Icons.Outlined.Close) { library.cancelDownload(entry.id) }
                                        entry.acquisition != null -> MenuAction(stringResource(R.string.download), Icons.Outlined.Download) { library.download(entry) }
                                        else -> null
                                    },
                                    MenuAction(stringResource(R.string.details), Icons.Outlined.Info) { library.showDetail(entry, entry.id) },
                                ),
                            )
                            else -> BookRow(
                                entry, local, progress, loader,
                                onClick = { library.showDetail(entry, entry.id) },
                                onDownload = { library.download(entry) }, onRead = openBook,
                                onCancel = { library.cancelDownload(entry.id) },
                            )
                        }
                    }
                    if (state.loadingMore) {
                        item(span = { GridItemSpan(maxLineSpan) }) {
                            Box(Modifier.fillMaxWidth().padding(Space.l), Alignment.Center) { Spinner() }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun FolderRow(entry: Entry, section: Section, onClick: () -> Unit) {
    val icon = if (section == Section.Authors) Icons.Outlined.Person else Icons.Outlined.Folder
    Column(Modifier.background(shelf.pane)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 72.dp).clickable(onClick = onClick).padding(horizontal = edgePadding, vertical = Space.m),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(40.dp).clip(RoundedCornerShape(20.dp)).background(shelf.soft), contentAlignment = Alignment.Center) {
                Icon(icon, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
            }
            Spacer(Modifier.width(Space.l))
            Column(Modifier.weight(1f)) {
                Text(entry.title, color = shelf.fg, fontSize = TextSize.subtitle, fontWeight = FontWeight.SemiBold, lineHeight = 22.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val count = entry.summary?.let { Regex("""^(\d+) books?$""").find(it.trim()) }?.groupValues?.get(1)?.toIntOrNull()
                val subtitle = count?.let { pluralStringResource(R.plurals.book_count, it, it) } ?: entry.summary
                subtitle?.let { Text(it, color = shelf.muted, fontSize = TextSize.label, lineHeight = 18.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            }
            Icon(Icons.Default.ChevronRight, null, tint = shelf.muted)
        }
        HorizontalDivider(color = shelf.line, modifier = Modifier.padding(start = edgePadding + 56.dp))
    }
}

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun BookRow(
    entry: Entry,
    local: LocalBook?,
    progress: Float?,
    loader: ImageLoader,
    onClick: () -> Unit,
    onDownload: () -> Unit,
    onRead: (LocalBook) -> Unit,
    onCancel: () -> Unit,
) {
    val format = entry.acquisition?.format
    Column(Modifier.background(shelf.pane)) {
        Row(
            Modifier.fillMaxWidth().clickable(onClickLabel = stringResource(R.string.details), onClick = onClick)
                .padding(start = edgePadding, end = Space.s, top = Space.m, bottom = Space.m),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Cover(entry.thumbnailUrl ?: entry.coverUrl, entry.title, format, loader, Modifier.size(width = 48.dp, height = 72.dp))
            Spacer(Modifier.width(Space.l))
            Column(Modifier.weight(1f)) {
                Text(entry.title, color = shelf.fg, fontSize = TextSize.subtitle, fontWeight = FontWeight.SemiBold, lineHeight = 22.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                if (entry.authors.isNotEmpty()) {
                    Text(entry.authors.joinToString(", "), color = shelf.muted, fontSize = TextSize.label, lineHeight = 18.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                // Wraps instead of squeezing, so large fonts stay readable
                FlowRow(
                    Modifier.padding(top = Space.xs),
                    horizontalArrangement = Arrangement.spacedBy(Space.s),
                    verticalArrangement = Arrangement.spacedBy(Space.xs),
                    itemVerticalAlignment = Alignment.CenterVertically,
                ) {
                    format?.let { Badge(it) }
                    formatSize(entry.acquisition?.length)?.let {
                        Text(it, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
                    }
                    if (local != null) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Outlined.DownloadDone, null, tint = shelf.muted, modifier = Modifier.size(14.dp))
                            Spacer(Modifier.width(Space.xs))
                            Text(stringResource(R.string.downloaded_short), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
                        }
                    }
                }
                if (progress != null) ReadBar(progress, Modifier.fillMaxWidth().padding(top = Space.s))
            }
            Spacer(Modifier.width(Space.s))
            // One kind of button for every state, so rows weigh the same
            when {
                local != null -> IconButton(onClick = { onRead(local) }) {
                    Icon(Icons.AutoMirrored.Outlined.MenuBook, stringResource(R.string.read_title, entry.title), tint = shelf.fg)
                }
                progress != null -> IconButton(onClick = onCancel) {
                    Box(contentAlignment = Alignment.Center) {
                        Spinner(size = 28.dp)
                        Icon(Icons.Outlined.Close, stringResource(R.string.cancel_download), tint = shelf.fg, modifier = Modifier.size(14.dp))
                    }
                }
                else -> IconButton(onClick = onDownload) {
                    Icon(Icons.Outlined.Download, stringResource(R.string.download_title, entry.title), tint = shelf.fg)
                }
            }
        }
        HorizontalDivider(color = shelf.line, modifier = Modifier.padding(start = edgePadding + 64.dp))
    }
}

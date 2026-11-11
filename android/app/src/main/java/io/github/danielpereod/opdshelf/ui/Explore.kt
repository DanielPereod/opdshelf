package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
    Row(Modifier.clip(RoundedCornerShape(4.dp)).border(1.dp, shelf.lineStrong, RoundedCornerShape(4.dp)).padding(2.dp)) {
        SwitchOption(Icons.AutoMirrored.Outlined.ViewList, stringResource(R.string.view_list), !grid) { onChange(false) }
        SwitchOption(Icons.Outlined.GridView, stringResource(R.string.view_grid), grid) { onChange(true) }
    }
}

@Composable
private fun SwitchOption(icon: ImageVector, label: String, selected: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.size(36.dp).clip(RoundedCornerShape(3.dp))
            .background(if (selected) shelf.soft else shelf.pane).clickable(role = Role.Tab, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, label, tint = if (selected) shelf.fg else shelf.muted, modifier = Modifier.size(18.dp)) }
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
        Column(Modifier.background(shelf.pane)) {
            SearchBox(
                value = query, onChange = { query = it }, placeholder = stringResource(R.string.search_hint),
                onSearch = {
                    if (query.isNotBlank()) {
                        focus.clearFocus()
                        library.search(query.trim())
                    }
                },
                modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 10.dp),
            )
            if (atRoot) {
                Row(
                    Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Section.entries.forEach { s -> Pill(stringResource(s.label), section == s) { library.selectSection(s) } }
                }
            } else {
                Spacer(Modifier.size(10.dp))
            }
            HorizontalDivider(color = shelf.line)
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
            entries.isEmpty() -> EmptyState(
                icon = Icons.Outlined.SearchOff,
                title = stringResource(if (state.label != null) R.string.no_results else R.string.empty_feed),
                text = stringResource(if (state.label != null) R.string.no_results_text else R.string.empty_feed_text),
            )
            else -> {
                val list = rememberLazyGridState()
                val nearEnd by remember {
                    derivedStateOf { (list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0) >= entries.size - 6 }
                }
                LaunchedEffect(nearEnd, entries.size) { if (nearEnd) library.loadMore() }
                val useGrid = grid && hasBooks
                LazyVerticalGrid(
                    state = list,
                    columns = if (useGrid) GridCells.Adaptive(104.dp) else GridCells.Fixed(1),
                    contentPadding = if (useGrid) PaddingValues(16.dp) else PaddingValues(0.dp),
                    horizontalArrangement = Arrangement.spacedBy(if (useGrid) 14.dp else 0.dp),
                    verticalArrangement = Arrangement.spacedBy(if (useGrid) 18.dp else 0.dp),
                    modifier = Modifier.fillMaxSize(),
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
                            )
                            else -> BookRow(
                                entry, local, progress, loader,
                                onClick = { library.showDetail(entry, entry.id) },
                                onDownload = { library.download(entry) }, onRead = openBook,
                            )
                        }
                    }
                    if (state.loadingMore) {
                        item(span = { GridItemSpan(maxLineSpan) }) {
                            Box(Modifier.fillMaxWidth().padding(16.dp), Alignment.Center) { Spinner() }
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
            Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(40.dp).clip(RoundedCornerShape(20.dp)).background(shelf.soft), contentAlignment = Alignment.Center) {
                Icon(icon, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
            }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(entry.title, color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val count = entry.summary?.let { Regex("""^(\d+) books?$""").find(it.trim()) }?.groupValues?.get(1)?.toIntOrNull()
                val subtitle = count?.let { pluralStringResource(R.plurals.book_count, it, it) } ?: entry.summary
                subtitle?.let { Text(it, color = shelf.muted, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            }
            Icon(Icons.Default.ChevronRight, null, tint = shelf.muted)
        }
        HorizontalDivider(color = shelf.line)
    }
}

@Composable
private fun BookRow(
    entry: Entry,
    local: LocalBook?,
    progress: Float?,
    loader: ImageLoader,
    onClick: () -> Unit,
    onDownload: () -> Unit,
    onRead: (LocalBook) -> Unit,
) {
    val format = entry.acquisition?.format
    Column(Modifier.background(shelf.pane)) {
        Row(
            Modifier.fillMaxWidth().clickable(onClickLabel = stringResource(R.string.details), onClick = onClick)
                .padding(start = 16.dp, end = 8.dp, top = 10.dp, bottom = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Cover(entry.thumbnailUrl ?: entry.coverUrl, entry.title, format, loader, Modifier.size(width = 40.dp, height = 60.dp))
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(entry.title, color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis)
                if (entry.authors.isNotEmpty()) {
                    Text(entry.authors.joinToString(", "), color = shelf.muted, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Row(Modifier.padding(top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    format?.let { Badge(it) }
                    formatSize(entry.acquisition?.length)?.let {
                        Spacer(Modifier.width(8.dp))
                        Text(it, color = shelf.muted, fontSize = 11.sp, fontFamily = Mono)
                    }
                    if (local != null) {
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.on_device), color = shelf.muted, fontSize = 11.sp)
                    }
                }
                if (progress != null) ReadBar(progress, Modifier.fillMaxWidth().padding(top = 8.dp))
            }
            Spacer(Modifier.width(8.dp))
            when {
                local != null -> PrimaryButton(stringResource(R.string.read), { onRead(local) }, height = 36.dp)
                progress != null -> Box(Modifier.size(48.dp), Alignment.Center) { Spinner(size = 20.dp) }
                else -> IconButton(onClick = onDownload) {
                    Icon(Icons.Outlined.Download, stringResource(R.string.download_title, entry.title), tint = shelf.fg)
                }
            }
        }
        HorizontalDivider(color = shelf.line)
    }
}

package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material.icons.automirrored.outlined.LibraryBooks
import androidx.compose.material.icons.automirrored.outlined.MenuBook
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.CollectionsBookmark
import androidx.compose.material.icons.outlined.DownloadDone
import androidx.compose.material.icons.outlined.FolderOff
import androidx.compose.material.icons.outlined.CreateNewFolder
import androidx.compose.material.icons.automirrored.outlined.DriveFileMove
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.PersonOff
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.DoneAll
import androidx.compose.material.icons.outlined.DriveFileRenameOutline
import androidx.compose.ui.semantics.selected
import io.github.danielpereod.opdshelf.data.folderBookCount
import io.github.danielpereod.opdshelf.data.folderPath
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
import androidx.compose.material.icons.outlined.UploadFile
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
import androidx.compose.animation.core.animateFloat
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.height
import io.github.danielpereod.opdshelf.app
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.ImageLoader
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.data.Entry
import io.github.danielpereod.opdshelf.data.LocalBook
import io.github.danielpereod.opdshelf.data.serverPath

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
@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun ExploreScreen(library: LibraryViewModel, openBook: (LocalBook) -> Unit, modifier: Modifier) {
    val stack by library.stack.collectAsState()
    val section by library.section.collectAsState()
    val downloads by library.downloads.collectAsState()
    val books by library.books.collectAsState()
    val remote by library.remoteReading.collectAsState()
    val loader = rememberImageLoader()
    val state = stack.lastOrNull()
    var query by rememberSaveable { mutableStateOf("") }
    var grid by rememberSaveable { mutableStateOf(true) }
    // In Folders, folders can be made and books moved between them
    val folder = library.currentFolder(stack)
    var newFolder by rememberSaveable { mutableStateOf(false) }
    var moving by remember { mutableStateOf<Entry?>(null) }
    val folders by library.folders.collectAsState()
    // Folders of the list being renamed, moved or deleted
    var renamingFolder by remember { mutableStateOf<String?>(null) }
    var movingFolder by remember { mutableStateOf<String?>(null) }
    var deletingFolder by remember { mutableStateOf<String?>(null) }
    // Books picked by a long press, in the feed they were picked in; empty means not selecting
    var picked by rememberSaveable { mutableStateOf(listOf<String>()) }
    var pickedIn by rememberSaveable { mutableStateOf<String?>(null) }
    var movingPicked by remember { mutableStateOf(false) }
    val selection = if (pickedIn != null && pickedIn == state?.url) picked else emptyList()
    val selecting = selection.isNotEmpty()
    fun clearSelection() { picked = emptyList(); pickedIn = null }
    fun toggle(entry: Entry) {
        if (pickedIn != state?.url) { picked = emptyList(); pickedIn = state?.url }
        picked = if (entry.id in picked) picked - entry.id else picked + entry.id
    }
    androidx.activity.compose.BackHandler(selecting) { clearSelection() }

    if (newFolder) {
        NewFolderDialog(onCreate = { newFolder = false; library.createFolder(it) }, onDismiss = { newFolder = false })
    }
    moving?.let { entry ->
        FolderPickerDialog(
            title = entry.title, folders = folders, current = entry.serverPath?.substringBeforeLast('/', ""),
            onPick = { moving = null; library.moveToFolder(entry, it) }, onDismiss = { moving = null },
        )
    }
    val entries = state?.feed?.entries.orEmpty()
    // Books moved away or gone after a refresh drop out of the selection
    LaunchedEffect(entries) {
        if (picked.isNotEmpty() && entries.isNotEmpty()) picked = picked.filter { id -> entries.any { it.id == id } }
    }
    val pickedEntries = entries.filter { it.id in selection && it.acquisition != null }
    if (movingPicked) {
        val parents = pickedEntries.mapNotNull { it.serverPath?.substringBeforeLast('/', "") }.distinct()
        FolderPickerDialog(
            title = pluralStringResource(R.plurals.book_count, pickedEntries.size, pickedEntries.size),
            folders = folders, current = parents.singleOrNull(),
            onPick = { movingPicked = false; library.moveToFolder(pickedEntries, it); clearSelection() },
            onDismiss = { movingPicked = false },
        )
    }
    renamingFolder?.let { path ->
        FolderNameDialog(
            title = stringResource(R.string.rename_folder), initial = path.substringAfterLast('/'),
            confirm = stringResource(R.string.rename),
            onDone = { renamingFolder = null; if (it != path.substringAfterLast('/')) library.renameFolder(path, it) },
            onDismiss = { renamingFolder = null },
        )
    }
    movingFolder?.let { path ->
        // A folder cannot go inside itself
        val choices = folders?.filterNot { it.path == path || it.path.startsWith("$path/") }
        FolderPickerDialog(
            title = path.substringAfterLast('/'), folders = choices, current = path.substringBeforeLast('/', ""),
            onPick = { movingFolder = null; library.moveFolder(path, it) }, onDismiss = { movingFolder = null },
            heading = stringResource(R.string.move_folder_to),
        )
    }
    deletingFolder?.let { path ->
        ConfirmDialog(
            title = stringResource(R.string.delete_folder_title, path.substringAfterLast('/')),
            text = stringResource(R.string.delete_folder_text),
            confirm = stringResource(R.string.delete),
            onConfirm = { deletingFolder = null; library.deleteFolder(path) },
            onDismiss = { deletingFolder = null },
        )
    }
    val focus = LocalFocusManager.current
    val pickBooks = androidx.activity.compose.rememberLauncherForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.OpenMultipleDocuments(),
    ) { library.prepareUpload(it) }
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
        val hasBooks = entries.any { it.acquisition != null }
        val atRoot = stack.size <= 1
        if (selecting) {
            val toDownload = pickedEntries.filter { e -> books.none { it.id == e.id } && !downloads.containsKey(e.id) }
            Header(
                pluralStringResource(R.plurals.selected_count, selection.size, selection.size),
                onBack = { clearSelection() },
                backIcon = Icons.Outlined.Close, backLabel = stringResource(R.string.clear_selection), titleMaxLines = 1,
                actions = {
                    if (toDownload.isNotEmpty()) {
                        IconButton(onClick = { toDownload.forEach { library.download(it) }; clearSelection() }) {
                            Icon(Icons.Outlined.Download, stringResource(R.string.download_selected), tint = shelf.fg)
                        }
                    }
                    if (pickedEntries.isNotEmpty() && pickedEntries.all { it.serverPath != null }) {
                        IconButton(onClick = { library.loadFolders(); movingPicked = true }) {
                            Icon(Icons.AutoMirrored.Outlined.DriveFileMove, stringResource(R.string.move_to_folder), tint = shelf.fg)
                        }
                    }
                },
            )
        } else Header(
            title = state?.label ?: if (atRoot) stringResource(R.string.tab_explore) else state?.feed?.title.orEmpty(),
            count = entries.size.takeIf { hasBooks && state?.feed?.nextUrl == null },
            onBack = if (!atRoot) ({ library.back() }) else null,
            actions = {
                if (folder != null && state?.error == null) {
                    IconButton(onClick = { newFolder = true }) {
                        Icon(Icons.Outlined.CreateNewFolder, stringResource(R.string.new_folder), tint = shelf.fg)
                    }
                }
                if (hasBooks) ViewSwitch(grid) { grid = it }
                IconButton(onClick = { pickBooks.launch(bookMimeTypes) }) {
                    Icon(Icons.Outlined.UploadFile, stringResource(R.string.add_books), tint = shelf.fg)
                }
            },
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
        // Pulling down reads the feed again; the books stay on screen meanwhile
        val pull = androidx.compose.material3.pulltorefresh.rememberPullToRefreshState()
        val refreshing = state?.refreshing == true
        androidx.compose.material3.pulltorefresh.PullToRefreshBox(
            isRefreshing = refreshing,
            onRefresh = { library.refresh() },
            state = pull,
            modifier = Modifier.fillMaxSize(),
            indicator = {
                androidx.compose.material3.pulltorefresh.PullToRefreshDefaults.Indicator(
                    state = pull, isRefreshing = refreshing, modifier = Modifier.align(Alignment.TopCenter),
                    containerColor = shelf.pane, color = shelf.fg,
                )
            },
        ) {
        when {
            state == null -> {}
            state.loading -> LoadingSkeleton(
                folders = atRoot && state.label == null && section in setOf(Section.Authors, Section.Series, Section.Folders),
                grid = grid,
            )
            // Without the server, the books on this device are still there to read
            state.error != null && atRoot && books.isNotEmpty() -> Column(Modifier.fillMaxSize()) {
                OfflineBand { library.retry() }
                LazyVerticalGrid(
                    columns = GridCells.Adaptive(gridCellWidth),
                    contentPadding = PaddingValues(horizontal = edgePadding, vertical = Space.l),
                    horizontalArrangement = Arrangement.spacedBy(if (isWide) Space.l else Space.m),
                    verticalArrangement = Arrangement.spacedBy(Space.xl),
                    modifier = Modifier.fillMaxSize(),
                ) {
                    items(books, key = { it.id }) { book ->
                        BookTile(
                            title = book.title, authors = book.authors,
                            cover = book.coverFile?.let { java.io.File(LocalContext.current.app.store.booksDir, it) },
                            format = book.format, loader = loader, onClick = { openBook(book) },
                            progress = book.percentage.takeIf { it > 0 }, finished = book.finished, downloaded = true,
                        )
                    }
                }
            }
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
                    folder != null && !atRoot -> Triple(Icons.Outlined.FolderOff, R.string.empty_folder, R.string.empty_folder_text)
                    !atRoot -> Triple(Icons.Outlined.FolderOff, R.string.empty_feed, R.string.empty_feed_text)
                    section == Section.Authors -> Triple(Icons.Outlined.PersonOff, R.string.empty_authors, R.string.empty_authors_text)
                    section == Section.Series -> Triple(Icons.Outlined.CollectionsBookmark, R.string.empty_series, R.string.empty_series_text)
                    section == Section.Folders -> Triple(Icons.Outlined.FolderOff, R.string.empty_folders, R.string.empty_folders_text)
                    else -> Triple(Icons.AutoMirrored.Outlined.LibraryBooks, R.string.empty_library, R.string.empty_library_text)
                }
                if (folder != null) {
                    EmptyState(
                        icon = icon, title = stringResource(title),
                        text = stringResource(if (atRoot) R.string.empty_folders_add else text),
                        action = stringResource(R.string.new_folder), onAction = { newFolder = true },
                    )
                } else {
                    EmptyState(icon = icon, title = stringResource(title), text = stringResource(text))
                }
            }
            else -> {
                val list = rememberLazyGridState()
                val nearEnd by remember {
                    derivedStateOf { (list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0) >= entries.size - 6 }
                }
                LaunchedEffect(nearEnd, entries.size) { if (nearEnd) library.loadMore() }
                val useGrid = grid && hasBooks
                val selectLabel = stringResource(R.string.select)
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
                        // How far it was read: on this device, or on any device for books not downloaded
                        val reading = entry.serverPath?.let { remote[it] }
                        val readFraction = local?.percentage?.takeIf { it > 0 } ?: reading?.percentage?.takeIf { it > 0 }
                        val finished = local?.finished ?: (reading?.finished == true)
                        val menu = listOfNotNull(
                            when {
                                local != null -> MenuAction(stringResource(R.string.read), Icons.AutoMirrored.Outlined.MenuBook) { openBook(local) }
                                progress != null -> MenuAction(stringResource(R.string.cancel_download), Icons.Outlined.Close) { library.cancelDownload(entry.id) }
                                entry.acquisition != null -> MenuAction(stringResource(R.string.download), Icons.Outlined.Download) { library.download(entry) }
                                else -> null
                            },
                            MenuAction(stringResource(R.string.details), Icons.Outlined.Info) { library.showDetail(entry, entry.id) },
                            entry.serverPath?.let {
                                MenuAction(stringResource(R.string.move_to_folder), Icons.AutoMirrored.Outlined.DriveFileMove) {
                                    library.loadFolders()
                                    moving = entry
                                }
                            },
                        )
                        val pickable = entry.acquisition != null
                        when {
                            feed != null -> {
                                val path = entry.folderPath
                                FolderRow(
                                    entry, section,
                                    onClick = { library.open(feed) },
                                    menu = if (path == null || selecting) emptyList() else listOfNotNull(
                                        MenuAction(stringResource(R.string.rename), Icons.Outlined.DriveFileRenameOutline) { renamingFolder = path },
                                        MenuAction(stringResource(R.string.move_to), Icons.AutoMirrored.Outlined.DriveFileMove) {
                                            library.loadFolders()
                                            movingFolder = path
                                        },
                                        // The server only deletes empty folders
                                        if (entry.folderBookCount == 0) {
                                            MenuAction(stringResource(R.string.delete_folder), Icons.Outlined.Delete, destructive = true) { deletingFolder = path }
                                        } else null,
                                    ),
                                )
                            }
                            useGrid -> BookTile(
                                title = entry.title, authors = entry.authors,
                                cover = entry.coverUrl ?: entry.thumbnailUrl, format = entry.acquisition?.format,
                                loader = loader,
                                onClick = { if (selecting && pickable) toggle(entry) else if (!selecting) library.showDetail(entry, entry.id) },
                                onLongClick = if (pickable) ({ toggle(entry) }) else null,
                                longClickLabel = selectLabel,
                                selected = if (selecting) entry.id in selection else null,
                                progress = readFraction, finished = finished,
                                downloaded = local != null, downloading = progress,
                                menu = menu,
                            )
                            else -> BookRow(
                                entry, local, progress, loader,
                                onClick = { if (selecting && pickable) toggle(entry) else if (!selecting) library.showDetail(entry, entry.id) },
                                onDownload = { library.download(entry) }, onRead = openBook,
                                onCancel = { library.cancelDownload(entry.id) },
                                onLongClick = if (pickable) ({ toggle(entry) }) else null,
                                selected = if (selecting) entry.id in selection else null,
                                readFraction = readFraction, finished = finished,
                                menu = menu,
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
}

/** A thin warning under the filters, with a way to try again. */
@Composable
private fun OfflineBand(onRetry: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().background(shelf.soft).padding(start = edgePadding, end = Space.xs, top = Space.xs, bottom = Space.xs)
            .semantics(mergeDescendants = true) { liveRegion = androidx.compose.ui.semantics.LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.CloudOff, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(Space.m))
        Text(stringResource(R.string.offline_band), color = shelf.fg, fontSize = TextSize.label, lineHeight = 18.sp, modifier = Modifier.weight(1f).padding(vertical = Space.s))
        androidx.compose.material3.TextButton(onClick = onRetry, modifier = Modifier.heightIn(min = 48.dp)) {
            Text(stringResource(R.string.retry), color = shelf.fg, fontSize = TextSize.label, fontWeight = FontWeight.SemiBold)
        }
    }
    HorizontalDivider(color = shelf.line)
}

/** Grey shapes where the books will be, pulsing gently while the server answers. */
@Composable
internal fun LoadingSkeleton(folders: Boolean, grid: Boolean) {
    val pulse = androidx.compose.animation.core.rememberInfiniteTransition(label = "skeleton")
    val alpha by pulse.animateFloat(
        0.5f, 1f,
        androidx.compose.animation.core.infiniteRepeatable(androidx.compose.animation.core.tween<Float>(800), androidx.compose.animation.core.RepeatMode.Reverse),
        label = "alpha",
    )
    val loading = stringResource(R.string.loading)
    val block = Modifier.graphicsLayer { this.alpha = alpha }.clip(RoundedCornerShape(4.dp)).background(shelf.line)
    val base = Modifier.fillMaxSize().semantics { contentDescription = loading }
    when {
        folders -> Column(base) {
            repeat(8) {
                Row(Modifier.fillMaxWidth().heightIn(min = 72.dp).padding(horizontal = edgePadding, vertical = Space.m), verticalAlignment = Alignment.CenterVertically) {
                    Box(block.size(40.dp).clip(RoundedCornerShape(20.dp)))
                    Spacer(Modifier.width(Space.l))
                    Column(Modifier.weight(1f)) {
                        Box(block.fillMaxWidth(0.6f).height(16.dp))
                        Spacer(Modifier.height(Space.s))
                        Box(block.fillMaxWidth(0.3f).height(12.dp))
                    }
                }
                HorizontalDivider(color = shelf.line, modifier = Modifier.padding(start = edgePadding + 56.dp))
            }
        }
        grid -> LazyVerticalGrid(
            columns = GridCells.Adaptive(gridCellWidth),
            contentPadding = PaddingValues(horizontal = edgePadding, vertical = Space.l),
            horizontalArrangement = Arrangement.spacedBy(if (isWide) Space.l else Space.m),
            verticalArrangement = Arrangement.spacedBy(Space.xl),
            userScrollEnabled = false, modifier = base,
        ) {
            items(18) {
                Column {
                    Box(block.fillMaxWidth().aspectRatio(2f / 3f))
                    Spacer(Modifier.height(Space.s))
                    Box(block.fillMaxWidth(0.9f).height(14.dp))
                    Spacer(Modifier.height(Space.xs))
                    Box(block.fillMaxWidth(0.6f).height(12.dp))
                }
            }
        }
        else -> Column(base) {
            repeat(8) {
                Row(Modifier.fillMaxWidth().padding(horizontal = edgePadding, vertical = Space.m), verticalAlignment = Alignment.CenterVertically) {
                    Box(block.size(width = 48.dp, height = 72.dp))
                    Spacer(Modifier.width(Space.l))
                    Column(Modifier.weight(1f)) {
                        Box(block.fillMaxWidth(0.7f).height(16.dp))
                        Spacer(Modifier.height(Space.s))
                        Box(block.fillMaxWidth(0.4f).height(12.dp))
                        Spacer(Modifier.height(Space.s))
                        Box(block.size(width = 40.dp, height = 16.dp))
                    }
                }
                HorizontalDivider(color = shelf.line, modifier = Modifier.padding(start = edgePadding + 64.dp))
            }
        }
    }
}

@Composable
private fun FolderRow(entry: Entry, section: Section, onClick: () -> Unit, menu: List<MenuAction> = emptyList()) {
    val icon = if (section == Section.Authors) Icons.Outlined.Person else Icons.Outlined.Folder
    // A long press opens the same menu as ⋮
    val menuOpen = remember { mutableStateOf(false) }
    Column(Modifier.background(shelf.pane)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 72.dp)
                .combinedClickable(
                    onClick = onClick,
                    onLongClick = if (menu.isNotEmpty()) ({ menuOpen.value = true }) else null,
                    onLongClickLabel = if (menu.isNotEmpty()) stringResource(R.string.more_options, entry.title) else null,
                )
                .padding(start = edgePadding, end = if (menu.isNotEmpty()) Space.xs else edgePadding, top = Space.m, bottom = Space.m),
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
            if (menu.isNotEmpty()) {
                MoreMenu(stringResource(R.string.more_options, entry.title), menu, state = menuOpen)
            } else {
                Icon(Icons.Default.ChevronRight, null, tint = shelf.muted)
            }
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
    onLongClick: (() -> Unit)? = null,
    /** Null outside selection mode; otherwise whether this row is picked. */
    selected: Boolean? = null,
    readFraction: Double? = null,
    finished: Boolean = false,
    menu: List<MenuAction> = emptyList(),
) {
    val format = entry.acquisition?.format
    Column(Modifier.background(if (selected == true) shelf.soft else shelf.pane)) {
        Row(
            Modifier.fillMaxWidth()
                .combinedClickable(
                    role = if (selected != null) Role.Checkbox else Role.Button,
                    onClickLabel = if (selected == null) stringResource(R.string.details) else null,
                    onClick = onClick,
                    onLongClick = onLongClick,
                    onLongClickLabel = onLongClick?.let { stringResource(R.string.select) },
                )
                .semantics { if (selected != null) this.selected = selected }
                .padding(start = edgePadding, end = Space.xs, top = Space.m, bottom = Space.m),
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
                    if (finished) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Outlined.DoneAll, null, tint = shelf.muted, modifier = Modifier.size(14.dp))
                            Spacer(Modifier.width(Space.xs))
                            Text(stringResource(R.string.finished), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
                        }
                    } else if (readFraction != null) {
                        Text(percent(readFraction), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
                    }
                }
                when {
                    progress != null -> ReadBar(progress, Modifier.fillMaxWidth().padding(top = Space.s))
                    finished -> ReadBar(1f, Modifier.fillMaxWidth().padding(top = Space.s))
                    readFraction != null -> ReadBar(readFraction.toFloat(), Modifier.fillMaxWidth().padding(top = Space.s))
                }
            }
            Spacer(Modifier.width(Space.s))
            if (selected != null) {
                // Selecting: a check in place of the buttons, as on the covers
                Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) {
                    Box(
                        Modifier.size(28.dp).clip(RoundedCornerShape(14.dp))
                            .background(if (selected) shelf.fg else shelf.pane)
                            .border(2.dp, shelf.fg, RoundedCornerShape(14.dp)),
                        contentAlignment = Alignment.Center,
                    ) {
                        if (selected) Icon(Icons.Outlined.Check, null, tint = shelf.inverse, modifier = Modifier.size(18.dp))
                    }
                }
            } else {
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
                if (menu.isNotEmpty()) MoreMenu(stringResource(R.string.more_options, entry.title), menu)
            }
        }
        HorizontalDivider(color = shelf.line, modifier = Modifier.padding(start = edgePadding + 64.dp))
    }
}

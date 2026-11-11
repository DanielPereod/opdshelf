package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ViewList
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.DownloadDone
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.GridView
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.ImageLoader
import coil.compose.AsyncImage
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.Entry
import io.github.danielpereod.opdshelf.data.LocalBook
import kotlinx.coroutines.launch
import java.io.File
import java.util.Locale

@Composable
private fun rememberImageLoader(): ImageLoader {
    val context = LocalContext.current
    return remember { ImageLoader.Builder(context).okHttpClient(context.app.api.client).crossfade(true).build() }
}

/** A cover, or a soft tile with the format on it when there is none (as on the web). */
@Composable
private fun Cover(model: Any?, label: String?, loader: ImageLoader, modifier: Modifier) {
    Box(
        modifier.clip(RoundedCornerShape(3.dp)).background(shelf.soft).border(1.dp, shelf.line, RoundedCornerShape(3.dp)),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            (label ?: "BOOK").uppercase(), color = shelf.muted, fontSize = 8.sp,
            fontWeight = FontWeight.Bold, letterSpacing = 0.5.sp,
        )
        if (model != null) {
            AsyncImage(
                model = model, imageLoader = loader, contentDescription = null,
                contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize(),
            )
        }
    }
}

private fun formatSize(bytes: Long?): String? {
    bytes ?: return null
    val units = listOf("B", "KB", "MB", "GB")
    var value = bytes.toDouble()
    var unit = 0
    while (value >= 1024 && unit < units.lastIndex) {
        value /= 1024
        unit++
    }
    return if (unit == 0) "$bytes B" else String.format(Locale.ROOT, "%.1f %s", value, units[unit])
}

/** The web's list/grid switch: two icons in a hairline box. */
@Composable
private fun ViewSwitch(grid: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.clip(RoundedCornerShape(4.dp)).border(1.dp, shelf.lineStrong, RoundedCornerShape(4.dp)).padding(2.dp),
    ) {
        SwitchOption(Icons.AutoMirrored.Outlined.ViewList, stringResource(R.string.view_list), !grid) { onChange(false) }
        SwitchOption(Icons.Outlined.GridView, stringResource(R.string.view_grid), grid) { onChange(true) }
    }
}

@Composable
private fun SwitchOption(icon: ImageVector, label: String, selected: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.size(30.dp).clip(RoundedCornerShape(3.dp))
            .background(if (selected) shelf.soft else shelf.pane).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, label, tint = if (selected) shelf.fg else shelf.muted, modifier = Modifier.size(18.dp)) }
}

// ---------------------------------------------------------------------------
// Library: the server's catalog

@Composable
fun CatalogScreen(library: LibraryViewModel, openBook: (LocalBook) -> Unit, modifier: Modifier) {
    val stack by library.stack.collectAsState()
    val downloads by library.downloads.collectAsState()
    val books by library.books.collectAsState()
    val loader = rememberImageLoader()
    val state = stack.lastOrNull()
    var query by rememberSaveable { mutableStateOf("") }
    var grid by rememberSaveable { mutableStateOf(false) }

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        val entries = state?.feed?.entries.orEmpty()
        val hasBooks = entries.any { it.acquisition != null }
        Header(
            title = state?.feed?.title ?: stringResource(R.string.tab_library),
            count = entries.size.takeIf { hasBooks },
            onBack = if (stack.size > 1) ({ library.back() }) else null,
            leading = if (stack.size <= 1) ({ Image(painterResource(R.drawable.logo), null, Modifier.size(22.dp)) }) else null,
            actions = { if (hasBooks) ViewSwitch(grid) { grid = it } },
        )
        Box(Modifier.background(shelf.pane).padding(horizontal = 16.dp, vertical = 10.dp)) {
            SearchBox(
                value = query, onChange = { query = it }, placeholder = stringResource(R.string.search),
                onSearch = { if (query.isNotBlank()) library.search(query.trim()) },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        HorizontalDivider(color = shelf.line)
        when {
            state == null -> {}
            state.loading -> Box(Modifier.fillMaxSize(), Alignment.Center) { Spinner() }
            state.error != null -> EmptyState(
                stringResource(R.string.cannot_load, state.error), stringResource(R.string.retry),
            ) { library.retry() }
            entries.isEmpty() -> EmptyState(stringResource(R.string.empty_feed))
            else -> {
                val list = rememberLazyGridState()
                val nearEnd by remember {
                    derivedStateOf { (list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0) >= entries.size - 6 }
                }
                LaunchedEffect(nearEnd, entries.size) { if (nearEnd) library.loadMore() }
                val useGrid = grid && hasBooks
                LazyVerticalGrid(
                    state = list,
                    columns = if (useGrid) GridCells.Adaptive(108.dp) else GridCells.Fixed(1),
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
                        val onClick = {
                            when {
                                feed != null -> library.open(feed)
                                local != null -> openBook(local)
                                progress == null -> library.download(entry)
                            }
                        }
                        when {
                            feed != null -> FolderRow(entry, onClick)
                            useGrid -> BookTile(entry, local, progress, loader, onClick)
                            else -> BookRow(entry, local, progress, loader, onClick, onDownload = { library.download(entry) }, onRead = openBook)
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
internal fun FolderRow(entry: Entry, onClick: () -> Unit) {
    Column(Modifier.background(shelf.pane)) {
        Row(
            Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                Modifier.size(36.dp).clip(RoundedCornerShape(4.dp)).background(shelf.soft),
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Outlined.Folder, null, tint = shelf.fg, modifier = Modifier.size(18.dp)) }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(entry.title, color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                entry.summary?.let { Text(it, color = shelf.muted, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            }
            Icon(Icons.Default.ChevronRight, null, tint = shelf.muted)
        }
        HorizontalDivider(color = shelf.line)
    }
}

@Composable
internal fun BookRow(
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
            Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Cover(entry.thumbnailUrl ?: entry.coverUrl, format, loader, Modifier.size(width = 36.dp, height = 52.dp))
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
                }
                if (progress != null) ReadBar(progress, Modifier.fillMaxWidth().padding(top = 8.dp))
            }
            Spacer(Modifier.width(8.dp))
            when {
                local != null -> PrimaryButton(stringResource(R.string.read), { onRead(local) }, height = 32.dp)
                progress != null -> Spinner(size = 20.dp)
                else -> IconButton(onClick = onDownload) {
                    Icon(Icons.Outlined.Download, stringResource(R.string.download), tint = shelf.fg)
                }
            }
        }
        HorizontalDivider(color = shelf.line)
    }
}

@Composable
internal fun BookTile(entry: Entry, local: LocalBook?, progress: Float?, loader: ImageLoader, onClick: () -> Unit) {
    Column(Modifier.clickable(onClick = onClick)) {
        Box {
            Cover(
                entry.coverUrl ?: entry.thumbnailUrl, entry.acquisition?.format, loader,
                Modifier.fillMaxWidth().aspectRatio(2f / 3f),
            )
            // Downloaded books carry a small mark, like a read state
            if (local != null || progress != null) {
                Box(
                    Modifier.align(Alignment.BottomEnd).padding(6.dp).size(24.dp).clip(RoundedCornerShape(4.dp)).background(shelf.fg),
                    contentAlignment = Alignment.Center,
                ) {
                    if (progress != null) {
                        Spinner(size = 14.dp)
                    } else {
                        Icon(Icons.Outlined.DownloadDone, null, tint = shelf.inverse, modifier = Modifier.size(14.dp))
                    }
                }
            }
        }
        if (progress != null) ReadBar(progress, Modifier.fillMaxWidth().padding(top = 6.dp))
        Spacer(Modifier.height(8.dp))
        Text(entry.title, color = shelf.fg, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis, lineHeight = 17.sp)
        if (entry.authors.isNotEmpty()) {
            Text(entry.authors.joinToString(", "), color = shelf.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

// ---------------------------------------------------------------------------
// On this device: downloaded books and how far they were read

@Composable
fun DeviceScreen(library: LibraryViewModel, openBook: (LocalBook) -> Unit, modifier: Modifier) {
    val books by library.books.collectAsState()
    val loader = rememberImageLoader()
    val context = LocalContext.current
    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(stringResource(R.string.tab_device), count = books.size.takeIf { it > 0 })
        if (books.isEmpty()) {
            EmptyState(stringResource(R.string.no_books_on_device))
            return
        }
        LazyColumn(Modifier.fillMaxSize()) {
            items(books.sortedByDescending { it.addedAt }, key = { it.id }) { book ->
                Column(Modifier.background(shelf.pane)) {
                    Row(
                        Modifier.fillMaxWidth().clickable { openBook(book) }.padding(start = 16.dp, end = 4.dp, top = 12.dp, bottom = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Cover(
                            book.coverFile?.let { File(context.app.store.booksDir, it) },
                            book.fileName.substringAfterLast('.', ""), loader,
                            Modifier.size(width = 44.dp, height = 64.dp),
                        )
                        Spacer(Modifier.width(14.dp))
                        Column(Modifier.weight(1f)) {
                            Text(book.title, color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis)
                            if (book.authors.isNotEmpty()) {
                                Text(book.authors.joinToString(", "), color = shelf.muted, fontSize = 13.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                                ReadBar(book.percentage.toFloat(), Modifier.weight(1f))
                                Spacer(Modifier.width(10.dp))
                                Text("${(book.percentage * 100).toInt()}%", color = shelf.muted, fontSize = 11.sp, fontFamily = Mono)
                            }
                        }
                        IconButton(onClick = { library.delete(book) }) {
                            Icon(Icons.Outlined.Delete, stringResource(R.string.remove_from_device), tint = shelf.muted)
                        }
                    }
                    HorizontalDivider(color = shelf.line)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Settings: laid out like the web's sign-in page

@Composable
fun SettingsScreen(modifier: Modifier, onSaved: () -> Unit) {
    val context = LocalContext.current
    val settings = context.app.settings
    var server by remember { mutableStateOf(settings.serverUrl) }
    var username by remember { mutableStateOf(settings.username) }
    var password by remember { mutableStateOf(settings.password) }
    var device by remember { mutableStateOf(settings.deviceName) }
    var checking by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    Column(
        modifier.fillMaxSize().background(shelf.bg).verticalScroll(rememberScrollState()).imePadding()
            .padding(horizontal = 20.dp, vertical = 28.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Image(painterResource(R.drawable.logo), null, Modifier.size(26.dp))
            Spacer(Modifier.width(10.dp))
            Text("OPDShelf", color = shelf.fg, fontSize = 22.sp, fontWeight = FontWeight.Bold)
        }
        Column(verticalArrangement = Arrangement.spacedBy(4.dp), modifier = Modifier.padding(top = 12.dp)) {
            Text(stringResource(R.string.tab_settings), color = shelf.fg, fontSize = 24.sp, fontWeight = FontWeight.Bold)
            Text(stringResource(R.string.settings_intro), color = shelf.muted, fontSize = 14.sp)
        }
        LabeledField(
            stringResource(R.string.server), server, { server = it },
            placeholder = "http://192.168.1.10:3000", keyboardType = KeyboardType.Uri,
        )
        LabeledField(stringResource(R.string.username), username, { username = it })
        LabeledField(
            stringResource(R.string.password), password, { password = it },
            keyboardType = KeyboardType.Password, visualTransformation = PasswordVisualTransformation(),
        )
        LabeledField(
            stringResource(R.string.device_name), device, { device = it },
            help = stringResource(R.string.device_name_help),
        )
        error?.let {
            Text(
                it, color = shelf.fg, fontSize = 13.sp,
                modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(4.dp)).background(shelf.soft)
                    .border(1.dp, shelf.lineStrong, RoundedCornerShape(4.dp)).padding(12.dp),
            )
        }
        PrimaryButton(
            text = stringResource(if (checking) R.string.checking else R.string.save),
            enabled = !checking && server.isNotBlank(),
            height = 44.dp,
            modifier = Modifier.fillMaxWidth(),
            onClick = {
                settings.serverUrl = server
                settings.username = username
                settings.password = password
                settings.deviceName = device
                server = settings.serverUrl
                checking = true
                error = null
                scope.launch {
                    try {
                        context.app.api.check()
                        context.app.sync.schedule()
                        onSaved()
                    } catch (e: Exception) {
                        error = context.getString(R.string.cannot_connect, context.describe(e))
                    } finally {
                        checking = false
                    }
                }
            },
        )
    }
}

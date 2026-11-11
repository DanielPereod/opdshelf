package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.MenuBook
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil.ImageLoader
import coil.compose.AsyncImage
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.Entry
import io.github.danielpereod.opdshelf.data.LocalBook
import kotlinx.coroutines.launch
import java.io.File

@Composable
private fun rememberImageLoader(): ImageLoader {
    val context = LocalContext.current
    return remember { ImageLoader.Builder(context).okHttpClient(context.app.api.client).build() }
}

@Composable
private fun Cover(model: Any?, loader: ImageLoader) {
    Box(Modifier.size(width = 48.dp, height = 68.dp)) {
        if (model != null) {
            AsyncImage(model = model, imageLoader = loader, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        } else {
            Icon(Icons.AutoMirrored.Filled.MenuBook, null, Modifier.align(Alignment.Center))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CatalogScreen(library: LibraryViewModel, openBook: (LocalBook) -> Unit, modifier: Modifier) {
    val stack by library.stack.collectAsState()
    val downloads by library.downloads.collectAsState()
    val books by library.books.collectAsState()
    val loader = rememberImageLoader()
    val state = stack.lastOrNull()
    var query by remember { mutableStateOf("") }

    Column(modifier.fillMaxSize()) {
        TopAppBar(
            title = { Text(state?.feed?.title ?: stringResource(R.string.tab_library), maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = {
                if (stack.size > 1) IconButton(onClick = { library.back() }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, null) }
            },
        )
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            placeholder = { Text(stringResource(R.string.search)) },
            leadingIcon = { Icon(Icons.Default.Search, null) },
            singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            keyboardActions = KeyboardActions(onSearch = { if (query.isNotBlank()) library.search(query.trim()) }),
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
        )
        when {
            state == null -> {}
            state.loading -> Box(Modifier.fillMaxSize(), Alignment.Center) { CircularProgressIndicator() }
            state.error != null -> Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
                Text(stringResource(R.string.cannot_load, state.error))
                TextButton(onClick = { library.retry() }) { Text(stringResource(R.string.retry)) }
            }
            else -> {
                val entries = state.feed?.entries.orEmpty()
                val list = rememberLazyListState()
                val nearEnd by remember { derivedStateOf { (list.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0) >= entries.size - 5 } }
                LaunchedEffect(nearEnd, entries.size) { if (nearEnd) library.loadMore() }
                if (entries.isEmpty()) {
                    Box(Modifier.fillMaxSize(), Alignment.Center) { Text(stringResource(R.string.empty_feed)) }
                }
                LazyColumn(state = list) {
                    items(entries, key = { it.id + it.feedUrl }) { entry ->
                        EntryRow(entry, books.firstOrNull { it.id == entry.id }, downloads[entry.id], loader,
                            onOpenFeed = { library.open(it) },
                            onDownload = { library.download(entry) },
                            onRead = openBook)
                        HorizontalDivider()
                    }
                    if (state.loadingMore) item { LinearProgressIndicator(Modifier.fillMaxWidth().padding(16.dp)) }
                }
            }
        }
    }
}

@Composable
private fun EntryRow(
    entry: Entry,
    local: LocalBook?,
    progress: Float?,
    loader: ImageLoader,
    onOpenFeed: (String) -> Unit,
    onDownload: () -> Unit,
    onRead: (LocalBook) -> Unit,
) {
    val feed = entry.feedUrl
    ListItem(
        modifier = Modifier.clickable {
            when {
                feed != null -> onOpenFeed(feed)
                local != null -> onRead(local)
                progress == null -> onDownload()
            }
        },
        leadingContent = if (feed == null) ({ Cover(entry.thumbnailUrl ?: entry.coverUrl, loader) }) else null,
        headlineContent = { Text(entry.title, maxLines = 2, overflow = TextOverflow.Ellipsis) },
        supportingContent = {
            Column {
                val line = listOfNotNull(
                    entry.authors.joinToString(", ").ifEmpty { null },
                    entry.acquisition?.format,
                ).joinToString(" · ")
                if (line.isNotEmpty()) Text(line, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (feed != null && entry.summary != null) Text(entry.summary, maxLines = 1)
                if (progress != null) LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth().padding(top = 6.dp))
            }
        },
        trailingContent = {
            when {
                feed != null -> Icon(Icons.Default.ChevronRight, null)
                local != null -> TextButton(onClick = { onRead(local) }) { Text(stringResource(R.string.read)) }
                progress != null -> CircularProgressIndicator(Modifier.size(24.dp))
                else -> IconButton(onClick = onDownload) { Icon(Icons.Default.Download, stringResource(R.string.download)) }
            }
        },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DeviceScreen(library: LibraryViewModel, openBook: (LocalBook) -> Unit, modifier: Modifier) {
    val books by library.books.collectAsState()
    val loader = rememberImageLoader()
    val context = LocalContext.current
    Column(modifier.fillMaxSize()) {
        TopAppBar(title = { Text(stringResource(R.string.tab_device)) })
        if (books.isEmpty()) {
            Box(Modifier.fillMaxSize().padding(32.dp), Alignment.Center) { Text(stringResource(R.string.no_books_on_device)) }
            return
        }
        LazyColumn {
            items(books.sortedByDescending { it.addedAt }, key = { it.id }) { book ->
                ListItem(
                    modifier = Modifier.clickable { openBook(book) },
                    leadingContent = { Cover(book.coverFile?.let { File(context.app.store.booksDir, it) }, loader) },
                    headlineContent = { Text(book.title, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                    supportingContent = {
                        Column {
                            if (book.authors.isNotEmpty()) Text(book.authors.joinToString(", "), maxLines = 1)
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                LinearProgressIndicator(progress = { book.percentage.toFloat() }, modifier = Modifier.weight(1f))
                                Spacer(Modifier.width(8.dp))
                                Text("${(book.percentage * 100).toInt()}%", style = MaterialTheme.typography.labelSmall)
                            }
                        }
                    },
                    trailingContent = {
                        IconButton(onClick = { library.delete(book) }) { Icon(Icons.Default.Delete, stringResource(R.string.remove_from_device)) }
                    },
                )
                HorizontalDivider()
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
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

    Column(modifier.fillMaxSize()) {
        TopAppBar(title = { Text(stringResource(R.string.tab_settings)) })
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.settings_intro), style = MaterialTheme.typography.bodyMedium)
            OutlinedTextField(server, { server = it }, label = { Text(stringResource(R.string.server)) },
                placeholder = { Text("http://192.168.1.10:3000") }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri), modifier = Modifier.fillMaxWidth())
            OutlinedTextField(username, { username = it }, label = { Text(stringResource(R.string.username)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(password, { password = it }, label = { Text(stringResource(R.string.password)) }, singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password), modifier = Modifier.fillMaxWidth())
            OutlinedTextField(device, { device = it }, label = { Text(stringResource(R.string.device_name)) },
                supportingText = { Text(stringResource(R.string.device_name_help)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
            error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            Button(
                enabled = !checking && server.isNotBlank(),
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
                            error = context.getString(R.string.cannot_connect, e.message ?: e.toString())
                        } finally {
                            checking = false
                        }
                    }
                },
            ) { Text(stringResource(if (checking) R.string.checking else R.string.save)) }
            Spacer(Modifier.height(8.dp))
        }
    }
}

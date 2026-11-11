package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
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
import androidx.compose.runtime.remember
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

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(
            stringResource(R.string.tab_home),
            leading = { Image(painterResource(R.drawable.logo), null, Modifier.size(24.dp)) },
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
        val current = remember(books) {
            books.filter { it.lastOpenedAt > 0 }.maxByOrNull { it.lastOpenedAt }
                ?: books.filter { it.percentage > 0 }.maxByOrNull { it.percentage }
        }
        val sorted = remember(books, sort) {
            when (sort) {
                Sort.Recent -> books.sortedByDescending { maxOf(it.lastOpenedAt, it.addedAt) }
                Sort.Title -> books.sortedBy { it.title.lowercase() }
                Sort.Progress -> books.sortedByDescending { it.percentage }
            }
        }
        val used = remember(books) { formatSize(context.app.store.usedBytes()) }
        LazyVerticalGrid(
            columns = GridCells.Adaptive(if (isWide) 128.dp else 104.dp),
            contentPadding = PaddingValues(horizontal = edgePadding, vertical = Space.l),
            horizontalArrangement = Arrangement.spacedBy(Space.l),
            verticalArrangement = Arrangement.spacedBy(Space.xl),
            modifier = Modifier.fillMaxSize(),
        ) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                SyncLine(status, onSync = { library.syncNow() })
            }
            if (current != null) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    ContinueCard(current, loader, onRead = { openBook(current) }, onDetails = { showDetail(current) })
                }
            }
            item(span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.padding(top = Space.s)) {
                    SectionTitle(
                        listOfNotNull(
                            pluralStringResource(R.plurals.books_on_device, books.size, books.size),
                            used,
                        ).joinToString(" · "),
                    )
                    Row(Modifier.horizontalScroll(rememberScrollState()).padding(top = Space.xs), horizontalArrangement = Arrangement.spacedBy(Space.s)) {
                        Sort.entries.forEach { Pill(stringResource(it.label), sort == it) { sort = it } }
                    }
                }
            }
            items(sorted, key = { it.id }) { book ->
                BookTile(
                    title = book.title, authors = book.authors,
                    cover = book.coverFile?.let { File(context.app.store.booksDir, it) },
                    format = book.format ?: book.fileName.substringAfterLast('.', ""),
                    loader = loader, progress = book.percentage,
                    onClick = { openBook(book) }, onLongClick = { showDetail(book) },
                )
            }
            item(span = { GridItemSpan(maxLineSpan) }) {
                Text(
                    stringResource(R.string.home_hint), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp,
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    modifier = Modifier.fillMaxWidth().padding(vertical = Space.s),
                )
            }
        }
    }
}

@Composable
private fun ContinueCard(book: LocalBook, loader: ImageLoader, onRead: () -> Unit, onDetails: () -> Unit) {
    val context = LocalContext.current
    val position = remember(book) { context.app.store.position(book.document)?.position }
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(shelf.pane)
            .border(1.dp, shelf.line, RoundedCornerShape(12.dp))
            .clickable(onClickLabel = stringResource(R.string.details), onClick = onDetails)
            .padding(Space.l),
    ) {
        SectionTitle(stringResource(if (book.percentage > 0) R.string.continue_reading else R.string.start_reading), Modifier.padding(top = 0.dp))
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

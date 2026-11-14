package io.github.danielpereod.opdshelf.ui

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.EditNote
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.Annotation
import io.github.danielpereod.opdshelf.data.LibraryHighlight
import io.github.danielpereod.opdshelf.data.LocalBook
import io.github.danielpereod.opdshelf.reader.ReaderActivity
import kotlinx.coroutines.launch

private sealed interface HighlightsState {
    data object Loading : HighlightsState
    data class Failed(val reason: String) : HighlightsState
    data class Ready(val review: List<LibraryHighlight>, val all: List<LibraryHighlight>) : HighlightsState
}

/**
 * Every highlight of the library, from every device, with a daily review of a
 * few of them on top. Read from the server; a highlight of a book on this
 * device opens the reader there.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun HighlightsScreen(library: LibraryViewModel, modifier: Modifier) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val books by library.books.collectAsState()
    var state by remember { mutableStateOf<HighlightsState>(HighlightsState.Loading) }
    var query by rememberSaveable { mutableStateOf("") }
    var onlyNotes by rememberSaveable { mutableStateOf(false) }
    var color by rememberSaveable { mutableStateOf<String?>(null) }
    var book by rememberSaveable { mutableStateOf<String?>(null) }
    var shown by rememberSaveable { mutableIntStateOf(PAGE) }

    fun load() {
        scope.launch {
            val api = context.app.api
            state = runCatching {
                HighlightsState.Ready(api.dailyReview(java.time.LocalDate.now().toString()), api.allHighlights())
            }.getOrElse { HighlightsState.Failed(it.message ?: it.toString()) }
        }
    }
    LaunchedEffect(Unit) { load() }

    val open: (LibraryHighlight) -> Unit = { h ->
        books.firstOrNull { it.document == h.document }?.let { local -> openAt(context, local, h) }
    }
    val share: (LibraryHighlight) -> Unit = { h ->
        val text = "“${h.text.trim()}”" + (h.title?.let { " — $it" } ?: "") + (if (h.hasNote) "\n\n${h.note.trim()}" else "")
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text), null))
    }
    val local = books.map { it.document }.toSet()

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        val all = (state as? HighlightsState.Ready)?.all
        Header(stringResource(R.string.tab_highlights), count = all?.size?.takeIf { it > 0 }, actions = {
            IconButton(onClick = { load() }) { Icon(Icons.Outlined.Refresh, stringResource(R.string.refresh), tint = shelf.fg) }
        })
        when (val s = state) {
            HighlightsState.Loading -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Spinner() }
            is HighlightsState.Failed -> EmptyState(
                stringResource(R.string.cannot_load_highlights, s.reason), stringResource(R.string.retry), { load() },
                icon = Icons.Outlined.CloudOff,
            )
            is HighlightsState.Ready -> if (s.all.isEmpty()) {
                EmptyState(stringResource(R.string.no_highlights_text), title = stringResource(R.string.no_highlights), icon = Icons.Outlined.EditNote)
            } else {
                val q = query.trim().lowercase()
                val filtered = s.all.filter { h ->
                    (book == null || h.document == book) && (!onlyNotes || h.hasNote) && (color == null || h.color == color) &&
                        (q.isEmpty() || "${h.text} ${h.note} ${h.chapter} ${h.title} ${h.authors}".lowercase().contains(q))
                }
                val used = Annotation.COLORS.keys.filter { c -> s.all.any { it.color == c } }
                val column = Modifier.widthIn(max = ReadableWidth).fillMaxWidth().padding(horizontal = edgePadding)
                LazyColumn(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally) {
                    if (s.review.isNotEmpty()) {
                        item {
                            Column(column.padding(top = Space.l)) {
                                SectionTitle(stringResource(R.string.daily_review))
                                Text(stringResource(R.string.daily_review_text), color = shelf.muted, fontSize = TextSize.bodySmall)
                                Spacer(Modifier.height(Space.m))
                            }
                        }
                        items(s.review, key = { "r" + it.id }) { h ->
                            HighlightCard(h, h.document in local, open, share, column.padding(bottom = Space.s))
                        }
                    }
                    item {
                        Column(column.padding(top = Space.xl)) {
                            SectionTitle(stringResource(R.string.library_highlights))
                            Text(
                                pluralStringResource(R.plurals.highlights_count, s.all.size, s.all.size, s.all.count { it.hasNote }),
                                color = shelf.muted, fontSize = TextSize.bodySmall,
                            )
                            Spacer(Modifier.height(Space.m))
                            SearchBox(query, { query = it; shown = PAGE }, stringResource(R.string.search_highlights), {}, Modifier.fillMaxWidth())
                            Spacer(Modifier.height(Space.s))
                            FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                                Pill(stringResource(R.string.all_highlights), !onlyNotes && color == null && book == null) {
                                    onlyNotes = false; color = null; book = null; shown = PAGE
                                }
                                book?.let { doc ->
                                    val title = s.all.firstOrNull { it.document == doc }?.title ?: stringResource(R.string.unknown_book)
                                    Pill("$title  ✕", true) { book = null; shown = PAGE }
                                }
                                Pill(stringResource(R.string.with_note), onlyNotes) { onlyNotes = !onlyNotes; shown = PAGE }
                                if (used.size > 1) used.forEach { c ->
                                    Pill(stringResource(colorNames[c] ?: R.string.color_yellow), color == c) {
                                        color = if (color == c) null else c; shown = PAGE
                                    }
                                }
                            }
                            Spacer(Modifier.height(Space.m))
                            if (filtered.isEmpty()) {
                                Text(
                                    stringResource(R.string.nothing_matches), color = shelf.muted, fontSize = TextSize.bodySmall,
                                    modifier = Modifier.padding(vertical = Space.xl).fillMaxWidth(),
                                    textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                                )
                            }
                        }
                    }
                    items(filtered.take(shown), key = { it.document + "/" + it.id }) { h ->
                        HighlightCard(
                            h, h.document in local, open, share, column.padding(bottom = Space.s),
                            onBook = { book = h.document; shown = PAGE }.takeIf { book == null },
                        )
                    }
                    if (filtered.size > shown) {
                        item {
                            SecondaryButton(stringResource(R.string.show_more), { shown += PAGE }, column.padding(vertical = Space.m))
                        }
                    }
                    item { Spacer(Modifier.height(Space.xl)) }
                }
            }
        }
    }
}

private const val PAGE = 100

private fun openAt(context: android.content.Context, book: LocalBook, h: LibraryHighlight) {
    context.startActivity(
        Intent(context, ReaderActivity::class.java)
            .putExtra(ReaderActivity.EXTRA_BOOK_ID, book.id)
            .putExtra(ReaderActivity.EXTRA_ANNOTATION, h.id),
    )
}

@Composable
private fun HighlightCard(
    h: LibraryHighlight,
    onDevice: Boolean,
    onOpen: (LibraryHighlight) -> Unit,
    onShare: (LibraryHighlight) -> Unit,
    modifier: Modifier,
    onBook: (() -> Unit)? = null,
) {
    val goTo = stringResource(R.string.go_to_highlight)
    Row(
        modifier.clip(RoundedCornerShape(8.dp)).background(shelf.pane).border(1.dp, shelf.line, RoundedCornerShape(8.dp))
            .then(if (onDevice) Modifier.clickable(onClickLabel = goTo) { onOpen(h) } else Modifier),
    ) {
        Box(Modifier.width(4.dp).heightIn(min = 72.dp).padding(vertical = Space.m).background(highlightColor(h.color)))
        Column(Modifier.weight(1f).padding(start = Space.m, top = Space.m, end = Space.xs, bottom = Space.xs)) {
            Text(
                h.text.trim().ifBlank { "—" }, color = shelf.fg, fontSize = TextSize.body, lineHeight = 23.sp, fontFamily = FontFamily.Serif,
                maxLines = 10, overflow = TextOverflow.Ellipsis,
            )
            if (h.hasNote) {
                Spacer(Modifier.height(Space.s))
                Text(
                    h.note.trim(), color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(shelf.soft).padding(Space.s),
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f).padding(vertical = Space.xs)) {
                    Text(
                        h.title ?: stringResource(R.string.unknown_book), color = shelf.fg, fontSize = TextSize.caption,
                        fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis,
                        modifier = if (onBook != null) Modifier.clickable(onClickLabel = stringResource(R.string.only_this_book), onClick = onBook) else Modifier,
                    )
                    Text(
                        listOfNotNull(h.authors?.takeIf { it.isNotBlank() }, h.chapter.takeIf { it.isNotBlank() }, relativeTime(h.created * 1000))
                            .joinToString(" · "),
                        color = shelf.muted, fontSize = TextSize.caption, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    )
                }
                IconButton(onClick = { onShare(h) }) {
                    Icon(Icons.Outlined.Share, stringResource(R.string.share_highlight), tint = shelf.muted, modifier = Modifier.size(20.dp))
                }
            }
        }
    }
}

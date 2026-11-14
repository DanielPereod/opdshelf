package io.github.danielpereod.opdshelf.reader

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.outlined.BookmarkBorder
import androidx.compose.material.icons.outlined.Search
import androidx.compose.ui.focus.focusRequester
import io.github.danielpereod.opdshelf.ui.TextSize
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.material.icons.outlined.SkipNext
import androidx.compose.material.icons.outlined.SkipPrevious
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.liveRegion
import io.github.danielpereod.opdshelf.ui.readerPalette
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import io.github.danielpereod.opdshelf.ui.Space
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.List
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.FormatQuote
import androidx.compose.material.icons.outlined.TextFields
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Slider
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.outlined.BrightnessHigh
import androidx.compose.material.icons.outlined.BrightnessLow
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.ui.Pill
import io.github.danielpereod.opdshelf.ui.PrimaryButton
import io.github.danielpereod.opdshelf.ui.SectionTitle
import io.github.danielpereod.opdshelf.ui.percent
import io.github.danielpereod.opdshelf.ui.shelf
import kotlinx.serialization.Serializable

/** Reading settings, kept between books. */
@Serializable
data class ReaderPrefs(
    val fontSize: Double = 1.0,
    /** 0 light, 1 sepia, 2 dark */
    val theme: Int = 0,
    /** 0 the book's own, 1 serif, 2 sans-serif */
    val font: Int = 0,
    /** 0 compact, 1 normal, 2 wide */
    val spacing: Int = 1,
    val scroll: Boolean = false,
    /** Volume down and up turn the page while the bars are hidden. */
    val volumeKeys: Boolean = true,
    val keepScreenOn: Boolean = true,
    /** 0 narrow, 1 normal, 2 wide */
    val margins: Int = 1,
    val justify: Boolean = false,
    /** Window brightness from 0 to 1, or below 0 to follow the system. */
    val brightness: Float = -1f,
)

/** A match of a search inside the book, with the words around it. */
data class SearchHit(
    val locator: org.readium.r2.shared.publication.Locator,
    val chapter: String?,
    val before: String,
    val match: String,
    val after: String,
)

/** An entry of the table of contents, flattened with its depth. */
data class TocItem(val title: String, val depth: Int, val href: String, val link: org.readium.r2.shared.publication.Link)

/** What the reader's controls show, updated by the activity as reading moves. */
class ReaderUi {
    var visible by mutableStateOf(false)
    var title by mutableStateOf("")
    var chapter by mutableStateOf<String?>(null)
    var progression by mutableFloatStateOf(0f)
    var page by mutableStateOf<Int?>(null)
    var pages by mutableStateOf<Int?>(null)
    /** Estimated minutes to the end of the chapter, when the book has positions. */
    var chapterMinutes by mutableStateOf<Int?>(null)
    var currentHref by mutableStateOf<String?>(null)
    var toc by mutableStateOf<List<TocItem>>(emptyList())
    var reflowable by mutableStateOf(false)
    var prefs by mutableStateOf(ReaderPrefs())
    var showHint by mutableStateOf(false)
    var sheet by mutableStateOf<Sheet?>(null)
    var bookmarks by mutableStateOf<List<io.github.danielpereod.opdshelf.data.Bookmark>>(emptyList())
    /** The bookmark of the page on screen, if it has one. */
    var currentBookmark by mutableStateOf<io.github.danielpereod.opdshelf.data.Bookmark?>(null)
    var searchable by mutableStateOf(false)
    var searchQuery by mutableStateOf("")
    var searchResults by mutableStateOf<List<SearchHit>>(emptyList())
    var searching by mutableStateOf(false)
    /** Whether a search ran, to tell "no matches" from "nothing searched yet". */
    var searched by mutableStateOf(false)
    /** The bookmark just removed, while it can still be put back. */
    var removedBookmark by mutableStateOf<io.github.danielpereod.opdshelf.data.Bookmark?>(null)

    /** Highlights and notes of the book, in reading order. */
    var annotations by mutableStateOf<List<io.github.danielpereod.opdshelf.data.Annotation>>(emptyList())
    /** The highlight whose note is being written or changed, if any. */
    var editing by mutableStateOf<io.github.danielpereod.opdshelf.data.Annotation?>(null)
    var editingNew by mutableStateOf(false)
    /** Which tab the contents sheet opens on: 0 contents, 1 bookmarks, 2 highlights. */
    var contentsTab by mutableStateOf<Int?>(null)

    enum class Sheet { Toc, Text, Search }
}

interface ReaderActions {
    fun close()
    fun goToProgression(progression: Double)
    fun goTo(item: TocItem)
    fun changePrefs(prefs: ReaderPrefs)
    fun dismissHint()
    fun toggleBookmark() {}
    fun goTo(bookmark: io.github.danielpereod.opdshelf.data.Bookmark) {}
    fun remove(bookmark: io.github.danielpereod.opdshelf.data.Bookmark) {}
    fun restore(bookmark: io.github.danielpereod.opdshelf.data.Bookmark) {}
    fun search(query: String) {}
    fun goTo(hit: SearchHit) {}
    fun goTo(annotation: io.github.danielpereod.opdshelf.data.Annotation) {}
    fun save(annotation: io.github.danielpereod.opdshelf.data.Annotation) {}
    fun delete(annotation: io.github.danielpereod.opdshelf.data.Annotation) {}
    fun shareAnnotations() {}
}

/** Controls over the book: shown and hidden with a tap in the middle of the page. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReaderOverlay(ui: ReaderUi, actions: ReaderActions) {
    Box(Modifier.fillMaxSize()) {
        AnimatedVisibility(
            ui.visible, enter = fadeIn() + slideInVertically { -it }, exit = fadeOut() + slideOutVertically { -it },
            modifier = Modifier.align(Alignment.TopCenter),
        ) { TopBar(ui, actions) }

        AnimatedVisibility(
            ui.visible, enter = fadeIn() + slideInVertically { it }, exit = fadeOut() + slideOutVertically { it },
            modifier = Modifier.align(Alignment.BottomCenter),
        ) { BottomBar(ui, actions) }

        AnimatedVisibility(
            ui.showHint && !ui.visible, enter = fadeIn(), exit = fadeOut(),
            modifier = Modifier.align(Alignment.BottomCenter),
        ) { MarkHint(actions) }
    }

    when (ui.sheet) {
        ReaderUi.Sheet.Toc -> ModalBottomSheet(onDismissRequest = { ui.sheet = null }, containerColor = shelf.pane) {
            ContentsSheet(ui, actions)
        }
        ReaderUi.Sheet.Search -> ModalBottomSheet(
            onDismissRequest = { ui.sheet = null }, containerColor = shelf.pane,
            sheetState = androidx.compose.material3.rememberModalBottomSheetState(skipPartiallyExpanded = true),
        ) {
            SearchSheet(ui, actions)
        }
        ReaderUi.Sheet.Text -> ModalBottomSheet(onDismissRequest = { ui.sheet = null }, containerColor = shelf.pane) {
            TextSheet(ui.prefs, actions::changePrefs)
        }
        null -> {}
    }

    ui.editing?.let { a ->
        io.github.danielpereod.opdshelf.ui.AnnotationEditor(
            a, ui.editingNew,
            onSave = { ui.editing = null; actions.save(it) },
            onDelete = { ui.editing = null; actions.delete(a) },
            onDismiss = { ui.editing = null },
        )
    }
}

@Composable
private fun TopBar(ui: ReaderUi, actions: ReaderActions) {
    Column(Modifier.fillMaxWidth().background(shelf.pane).statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = actions::close) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.close_book), tint = shelf.fg)
            }
            Column(Modifier.weight(1f).padding(horizontal = 4.dp)) {
                Text(ui.title, color = shelf.fg, fontSize = TextSize.subtitle, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                ui.chapter?.takeIf { it.isNotBlank() }?.let {
                    Text(it, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (ui.searchable) {
                IconButton(onClick = { ui.sheet = ReaderUi.Sheet.Search }) {
                    Icon(Icons.Outlined.Search, stringResource(R.string.search_book), tint = shelf.fg)
                }
            }
            IconButton(onClick = actions::toggleBookmark) {
                val marked = ui.currentBookmark != null
                Icon(
                    if (marked) Icons.Filled.Bookmark else Icons.Outlined.BookmarkBorder,
                    stringResource(if (marked) R.string.remove_bookmark else R.string.add_bookmark), tint = shelf.fg,
                )
            }
            if (ui.toc.isNotEmpty() || ui.bookmarks.isNotEmpty() || ui.reflowable || ui.annotations.isNotEmpty()) {
                IconButton(onClick = { ui.sheet = ReaderUi.Sheet.Toc }) {
                    Icon(Icons.AutoMirrored.Outlined.List, stringResource(R.string.contents), tint = shelf.fg)
                }
            }
            if (ui.reflowable) {
                IconButton(onClick = { ui.sheet = ReaderUi.Sheet.Text }) {
                    Icon(Icons.Outlined.TextFields, stringResource(R.string.text_settings), tint = shelf.fg)
                }
            }
        }
        HorizontalDivider(color = shelf.line)
    }
}

@Composable
private fun BottomBar(ui: ReaderUi, actions: ReaderActions) {
    var dragging by remember { mutableStateOf<Float?>(null) }
    val shown = dragging ?: ui.progression
    val current = ui.toc.indexOfLast { it.href.substringBefore('#') == ui.currentHref?.substringBefore('#') }
    val previous = ui.toc.getOrNull(current - 1)
    val next = ui.toc.getOrNull(current + 1)
    val progressLabel = stringResource(R.string.book_progress)
    val progressValue = percent(shown.toDouble())
    Column(Modifier.fillMaxWidth().background(shelf.pane).navigationBarsPadding()) {
        HorizontalDivider(color = shelf.line)
        Column(Modifier.padding(horizontal = Space.s, vertical = Space.s)) {
          Row(verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { previous?.let(actions::goTo) }, enabled = previous != null) {
                Icon(Icons.Outlined.SkipPrevious, stringResource(R.string.prev_chapter), tint = if (previous != null) shelf.fg else shelf.line)
            }
            Slider(
                modifier = Modifier.weight(1f).semantics {
                    contentDescription = progressLabel
                    stateDescription = progressValue
                },
                value = shown,
                onValueChange = { dragging = it },
                onValueChangeFinished = {
                    dragging?.let { actions.goToProgression(it.toDouble()) }
                    dragging = null
                },
                colors = SliderDefaults.colors(
                    thumbColor = shelf.fg, activeTrackColor = shelf.fg, inactiveTrackColor = shelf.soft,
                    activeTickColor = shelf.fg, inactiveTickColor = shelf.soft,
                ),
            )
            IconButton(onClick = { next?.let(actions::goTo) }, enabled = next != null) {
                Icon(Icons.Outlined.SkipNext, stringResource(R.string.next_chapter), tint = if (next != null) shelf.fg else shelf.line)
            }
          }
            Row(Modifier.fillMaxWidth().padding(horizontal = Space.l), horizontalArrangement = Arrangement.SpaceBetween) {
                val page = ui.page
                val pages = ui.pages
                // The chapter is already in the top bar: here, where in the book
                val minutes = ui.chapterMinutes
                val left = when {
                    minutes == null -> null
                    minutes < 1 -> stringResource(R.string.minute_left)
                    else -> stringResource(R.string.minutes_left, minutes)
                }
                Text(
                    listOfNotNull(
                        if (page != null && pages != null) stringResource(R.string.page_of, page, pages) else null,
                        left,
                    ).joinToString(" · "),
                    color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(Space.l))
                Text(percent(shown.toDouble()), color = shelf.fg, fontSize = TextSize.caption, lineHeight = 16.sp, fontWeight = FontWeight.SemiBold)
            }
        }
    }
}

@Composable
private fun MarkHint(actions: ReaderActions) {
    Row(
        Modifier.navigationBarsPadding().padding(Space.l).widthIn(max = 560.dp).fillMaxWidth().clip(RoundedCornerShape(12.dp))
            .background(shelf.fg).padding(start = Space.l, top = Space.s, bottom = Space.s, end = Space.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.FormatQuote, null, tint = shelf.inverse, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(12.dp))
        Text(stringResource(R.string.mark_hint), color = shelf.inverse, fontSize = TextSize.label, lineHeight = 20.sp, modifier = Modifier.weight(1f))
        IconButton(onClick = actions::dismissHint) {
            Icon(Icons.Outlined.Close, stringResource(R.string.got_it), tint = shelf.inverse)
        }
    }
}

@Composable
internal fun TocSheet(ui: ReaderUi, showTitle: Boolean = true, onPick: (TocItem) -> Unit) {
    val current = ui.toc.indexOfLast { it.href.substringBefore('#') == ui.currentHref?.substringBefore('#') }
    val list = rememberLazyListState()
    LaunchedEffect(Unit) { if (current > 2) list.scrollToItem(current - 2) }
    Column(Modifier.padding(bottom = 16.dp)) {
        if (showTitle) Text(
            stringResource(R.string.contents), color = shelf.fg, fontSize = 18.sp, fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(horizontal = Space.xl, vertical = Space.s).semantics { heading() },
        )
        LazyColumn(state = list) {
            itemsIndexed(ui.toc) { index, item ->
                val selected = index == current
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 48.dp)
                        .selectable(selected = selected, onClick = { onPick(item) })
                        .background(if (selected) shelf.soft else shelf.pane)
                        .height(androidx.compose.foundation.layout.IntrinsicSize.Min),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // The chapter being read: a bar at the edge besides the shade
                    Box(Modifier.width(4.dp).fillMaxHeight().background(if (selected) shelf.fg else Color.Transparent))
                    Spacer(Modifier.width((20 + item.depth * 16).dp))
                    Text(
                        item.title, color = shelf.fg, fontSize = TextSize.body, lineHeight = 22.sp,
                        fontWeight = if (selected || item.depth == 0) FontWeight.SemiBold else FontWeight.Normal,
                        modifier = Modifier.weight(1f).padding(vertical = 12.dp),
                    )
                    Spacer(Modifier.width(Space.xl))
                }
            }
        }
    }
}

@Composable
internal fun TextSheet(prefs: ReaderPrefs, onChange: (ReaderPrefs) -> Unit) {
    Column(Modifier.verticalScroll(androidx.compose.foundation.rememberScrollState()).padding(start = Space.xl, end = Space.xl, bottom = Space.xxl)) {
        Text(
            stringResource(R.string.text_title), color = shelf.fg, fontSize = 18.sp, fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(vertical = Space.s).semantics { heading() },
        )
        SectionTitle(stringResource(R.string.text_size))
        Row(verticalAlignment = Alignment.CenterVertically) {
            SizeButton("A−", stringResource(R.string.text_smaller)) { onChange(prefs.copy(fontSize = (prefs.fontSize - 0.1).coerceAtLeast(0.6))) }
            Text(
                "${(prefs.fontSize * 100).toInt()}\u00A0%", color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f), textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
            SizeButton("A+", stringResource(R.string.text_larger)) { onChange(prefs.copy(fontSize = (prefs.fontSize + 0.1).coerceAtMost(2.5))) }
        }
        Spacer(Modifier.height(Space.s))
        SectionTitle(stringResource(R.string.theme))
        // Swatches of the page itself, so the choice is seen, not read
        Row(horizontalArrangement = Arrangement.spacedBy(Space.m)) {
            listOf(R.string.theme_light, R.string.theme_sepia, R.string.theme_dark).forEachIndexed { i, label ->
                ThemeSwatch(stringResource(label), readerPalette(i), prefs.theme == i) { onChange(prefs.copy(theme = i)) }
            }
        }
        Spacer(Modifier.height(Space.s))
        SectionTitle(stringResource(R.string.font))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(R.string.font_book, R.string.font_serif, R.string.font_sans).forEachIndexed { i, label ->
                // Each written in its own letter
                val family = when (i) {
                    1 -> androidx.compose.ui.text.font.FontFamily.Serif
                    2 -> androidx.compose.ui.text.font.FontFamily.SansSerif
                    else -> null
                }
                Pill(stringResource(label), prefs.font == i, fontFamily = family) { onChange(prefs.copy(font = i)) }
            }
        }
        Spacer(Modifier.height(Space.s))
        SectionTitle(stringResource(R.string.line_spacing))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(R.string.spacing_compact, R.string.spacing_normal, R.string.spacing_wide).forEachIndexed { i, label ->
                Pill(stringResource(label), prefs.spacing == i) { onChange(prefs.copy(spacing = i)) }
            }
        }
        Spacer(Modifier.height(Space.s))
        SectionTitle(stringResource(R.string.margins))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(R.string.margins_narrow, R.string.spacing_normal, R.string.margins_wide).forEachIndexed { i, label ->
                Pill(stringResource(label), prefs.margins == i) { onChange(prefs.copy(margins = i)) }
            }
        }
        Spacer(Modifier.height(Space.s))
        ToggleRow(stringResource(R.string.justify), null, prefs.justify) { onChange(prefs.copy(justify = it)) }
        Spacer(Modifier.height(Space.s))
        HorizontalDivider(color = shelf.line)
        Spacer(Modifier.height(Space.s))
        ToggleRow(stringResource(R.string.brightness_system), null, prefs.brightness < 0) {
            onChange(prefs.copy(brightness = if (it) -1f else 0.6f))
        }
        if (prefs.brightness >= 0) {
            val label = stringResource(R.string.brightness)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.BrightnessLow, null, tint = shelf.muted, modifier = Modifier.size(20.dp))
                Slider(
                    value = prefs.brightness, valueRange = 0.02f..1f,
                    onValueChange = { onChange(prefs.copy(brightness = it)) },
                    modifier = Modifier.weight(1f).padding(horizontal = Space.s).semantics {
                        contentDescription = label
                        stateDescription = percent(prefs.brightness.toDouble())
                    },
                    colors = SliderDefaults.colors(thumbColor = shelf.fg, activeTrackColor = shelf.fg, inactiveTrackColor = shelf.soft),
                )
                Icon(Icons.Outlined.BrightnessHigh, null, tint = shelf.muted, modifier = Modifier.size(20.dp))
            }
        }
        Spacer(Modifier.height(Space.s))
        HorizontalDivider(color = shelf.line)
        Spacer(Modifier.height(Space.s))
        ToggleRow(stringResource(R.string.volume_keys), null, prefs.volumeKeys) { onChange(prefs.copy(volumeKeys = it)) }
        ToggleRow(stringResource(R.string.keep_screen_on), null, prefs.keepScreenOn) { onChange(prefs.copy(keepScreenOn = it)) }
        Row(
            Modifier.fillMaxWidth().heightIn(min = 56.dp)
                .toggleable(value = prefs.scroll, role = androidx.compose.ui.semantics.Role.Switch) { onChange(prefs.copy(scroll = it)) }
                .padding(vertical = Space.s),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.scroll_mode), color = shelf.fg, fontSize = TextSize.body, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold)
                Text(stringResource(R.string.scroll_mode_help), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
            }
            Spacer(Modifier.width(Space.l))
            Switch(
                checked = prefs.scroll, onCheckedChange = null,
                colors = SwitchDefaults.colors(
                    checkedThumbColor = shelf.inverse, checkedTrackColor = shelf.fg,
                    uncheckedThumbColor = shelf.muted, uncheckedTrackColor = shelf.soft, uncheckedBorderColor = shelf.lineStrong,
                ),
            )
        }
    }
}

@Composable
private fun SizeButton(label: String, description: String, onClick: () -> Unit) {
    Box(
        Modifier.size(width = 72.dp, height = 48.dp).clip(RoundedCornerShape(8.dp)).background(shelf.soft)
            .clickable(role = androidx.compose.ui.semantics.Role.Button, onClick = onClick)
            .clearAndSetSemantics {
                contentDescription = description
                role = androidx.compose.ui.semantics.Role.Button
                onClick { onClick(); true }
            },
        contentAlignment = Alignment.Center,
    ) { Text(label, color = shelf.fg, fontSize = 16.sp, fontWeight = FontWeight.Bold) }
}


@Composable
private fun ThemeSwatch(label: String, colors: io.github.danielpereod.opdshelf.ui.ShelfColors, selected: Boolean, onClick: () -> Unit) {
    Column(
        Modifier.clip(RoundedCornerShape(8.dp))
            .selectable(selected = selected, role = androidx.compose.ui.semantics.Role.RadioButton, onClick = onClick)
            .padding(Space.xs),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            Modifier.size(width = 72.dp, height = 48.dp).clip(RoundedCornerShape(8.dp)).background(colors.bg)
                .border(if (selected) 2.dp else 1.dp, if (selected) shelf.fg else shelf.lineStrong, RoundedCornerShape(8.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Text(stringResource(R.string.theme_sample), color = colors.fg, fontSize = 18.sp, fontWeight = FontWeight.SemiBold)
        }
        Spacer(Modifier.height(Space.xs))
        Text(
            label, color = shelf.fg, fontSize = TextSize.caption, lineHeight = 16.sp,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
        )
    }
}

@Composable
private fun ToggleRow(title: String, help: String?, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 56.dp)
            .toggleable(value = checked, role = androidx.compose.ui.semantics.Role.Switch, onValueChange = onChange)
            .padding(vertical = Space.s),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, color = shelf.fg, fontSize = TextSize.body, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold)
            help?.let { Text(it, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp) }
        }
        Spacer(Modifier.width(Space.l))
        Switch(
            checked = checked, onCheckedChange = null,
            colors = SwitchDefaults.colors(
                checkedThumbColor = shelf.inverse, checkedTrackColor = shelf.fg,
                uncheckedThumbColor = shelf.muted, uncheckedTrackColor = shelf.soft, uncheckedBorderColor = shelf.lineStrong,
            ),
        )
    }
}

/** Contents and bookmarks, as two tabs of one sheet. */
@Composable
internal fun ContentsSheet(ui: ReaderUi, actions: ReaderActions) {
    var tab by remember { mutableStateOf(ui.contentsTab ?: if (ui.toc.isEmpty()) 1 else 0) }
    Column {
        androidx.compose.material3.PrimaryTabRow(
            selectedTabIndex = tab, containerColor = shelf.pane, contentColor = shelf.fg,
            indicator = {
                androidx.compose.material3.TabRowDefaults.PrimaryIndicator(
                    Modifier.tabIndicatorOffset(tab, matchContentSize = true), color = shelf.fg,
                )
            },
            divider = { HorizontalDivider(color = shelf.line) },
        ) {
            listOf(R.string.contents, R.string.bookmarks, R.string.highlights).forEachIndexed { i, label ->
                androidx.compose.material3.Tab(
                    selected = tab == i, onClick = { tab = i },
                    selectedContentColor = shelf.fg, unselectedContentColor = shelf.muted,
                    text = {
                        val n = when {
                            i == 1 && ui.bookmarks.isNotEmpty() -> " ${ui.bookmarks.size}"
                            i == 2 && ui.annotations.isNotEmpty() -> " ${ui.annotations.size}"
                            else -> ""
                        }
                        Text(stringResource(label) + n, fontSize = TextSize.bodySmall, fontWeight = FontWeight.SemiBold)
                    },
                )
            }
        }
        if (tab == 0) {
            TocSheet(ui, showTitle = false) {
                ui.sheet = null
                actions.goTo(it)
            }
        } else if (tab == 1) {
            BookmarksList(ui, actions)
        } else {
            io.github.danielpereod.opdshelf.ui.AnnotationList(
                ui.annotations,
                onOpen = { ui.sheet = null; actions.goTo(it) },
                onEdit = { ui.editingNew = false; ui.editing = it },
                onDelete = actions::delete,
                onShare = actions::shareAnnotations,
                modifier = Modifier.verticalScroll(androidx.compose.foundation.rememberScrollState())
                    .padding(horizontal = Space.l, vertical = Space.s),
            )
        }
    }
}

@Composable
private fun BookmarksList(ui: ReaderUi, actions: ReaderActions) {
    // A removed bookmark can be put back for a few seconds, right where it was taken
    ui.removedBookmark?.let { gone ->
        LaunchedEffect(gone) {
            kotlinx.coroutines.delay(8000)
            if (ui.removedBookmark == gone) ui.removedBookmark = null
        }
        Row(
            Modifier.padding(horizontal = Space.l, vertical = Space.s).fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(shelf.fg)
                .padding(start = Space.l, end = Space.xs)
                .semantics(mergeDescendants = true) { liveRegion = androidx.compose.ui.semantics.LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(stringResource(R.string.bookmark_removed), color = shelf.inverse, fontSize = TextSize.label, lineHeight = 18.sp, modifier = Modifier.weight(1f))
            androidx.compose.material3.TextButton(onClick = { actions.restore(gone) }, modifier = Modifier.heightIn(min = 48.dp)) {
                Text(stringResource(R.string.undo), color = shelf.inverse, fontSize = TextSize.label, fontWeight = FontWeight.Bold)
            }
        }
    }
    if (ui.bookmarks.isEmpty()) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.xxl),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Icon(Icons.Outlined.BookmarkBorder, null, tint = shelf.muted, modifier = Modifier.size(32.dp))
            Spacer(Modifier.height(Space.m))
            Text(stringResource(R.string.no_bookmarks), color = shelf.fg, fontSize = TextSize.subtitle, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(Space.xs))
            Text(
                stringResource(R.string.no_bookmarks_text), color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp,
                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
        }
        return
    }
    LazyColumn(Modifier.padding(bottom = Space.l)) {
        itemsIndexed(ui.bookmarks.sortedBy { it.progression }) { _, mark ->
            Row(
                Modifier.fillMaxWidth().heightIn(min = 56.dp)
                    .clickable { ui.sheet = null; actions.goTo(mark) }
                    .padding(start = Space.xl, end = Space.xs, top = Space.s, bottom = Space.s),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Filled.Bookmark, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(Space.l))
                Column(Modifier.weight(1f)) {
                    Text(
                        mark.chapter ?: "—", color = shelf.fg, fontSize = TextSize.body, lineHeight = 22.sp,
                        fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        listOfNotNull(percent(mark.progression), mark.text?.takeIf { it.isNotBlank() }?.let { "«${it.trim()}»" }).joinToString(" · "),
                        color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 2, overflow = TextOverflow.Ellipsis,
                    )
                }
                IconButton(onClick = { actions.remove(mark) }) {
                    Icon(Icons.Outlined.Close, stringResource(R.string.remove_bookmark), tint = shelf.muted)
                }
            }
        }
    }
}

/** Search inside the book: a box, then each match with the words around it. */
@Composable
internal fun SearchSheet(ui: ReaderUi, actions: ReaderActions) {
    val focus = remember { androidx.compose.ui.focus.FocusRequester() }
    LaunchedEffect(Unit) { if (ui.searchResults.isEmpty()) runCatching { focus.requestFocus() } }
    Column(Modifier.fillMaxWidth().fillMaxHeight(0.9f)) {
        Text(
            stringResource(R.string.search_book), color = shelf.fg, fontSize = 18.sp, fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(horizontal = Space.xl, vertical = Space.s).semantics { heading() },
        )
        io.github.danielpereod.opdshelf.ui.SearchBox(
            ui.searchQuery, { ui.searchQuery = it }, stringResource(R.string.search_book_hint),
            onSearch = { if (ui.searchQuery.isNotBlank()) actions.search(ui.searchQuery.trim()) },
            modifier = Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.s).focusRequester(focus),
        )
        when {
            ui.searching && ui.searchResults.isEmpty() -> Row(
                Modifier.fillMaxWidth().padding(Space.xl), verticalAlignment = Alignment.CenterVertically,
            ) {
                io.github.danielpereod.opdshelf.ui.Spinner(size = 20.dp)
                Spacer(Modifier.width(Space.m))
                Text(stringResource(R.string.searching), color = shelf.muted, fontSize = TextSize.bodySmall)
            }
            ui.searched && ui.searchResults.isEmpty() -> Column(Modifier.fillMaxWidth().padding(Space.xl)) {
                Text(stringResource(R.string.no_matches), color = shelf.fg, fontSize = TextSize.subtitle, fontWeight = FontWeight.SemiBold)
                Text(stringResource(R.string.no_matches_text), color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp)
            }
            ui.searchResults.isNotEmpty() -> {
                val n = ui.searchResults.size
                Text(
                    if (n >= SEARCH_LIMIT) stringResource(R.string.search_count_more, SEARCH_LIMIT) else stringResource(R.string.search_count, n),
                    color = shelf.muted, fontSize = TextSize.caption, modifier = Modifier.padding(horizontal = Space.xl, vertical = Space.xs),
                )
                LazyColumn {
                    itemsIndexed(ui.searchResults) { _, hit ->
                        Column(
                            Modifier.fillMaxWidth().heightIn(min = 56.dp)
                                .clickable { ui.sheet = null; actions.goTo(hit) }
                                .padding(horizontal = Space.xl, vertical = Space.m),
                        ) {
                            hit.chapter?.let {
                                Text(it, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                            Text(
                                androidx.compose.ui.text.buildAnnotatedString {
                                    append("…" + hit.before.takeLast(60))
                                    pushStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.Bold, background = shelf.soft))
                                    append(hit.match)
                                    pop()
                                    append(hit.after.take(60) + "…")
                                },
                                color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp, maxLines = 3, overflow = TextOverflow.Ellipsis,
                            )
                        }
                        HorizontalDivider(color = shelf.line, modifier = Modifier.padding(horizontal = Space.xl))
                    }
                }
            }
        }
    }
}

/** Searches stop after this many matches: enough to find a passage. */
const val SEARCH_LIMIT = 100

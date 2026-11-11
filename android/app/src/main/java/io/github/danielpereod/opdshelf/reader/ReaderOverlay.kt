package io.github.danielpereod.opdshelf.reader

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
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
    var currentHref by mutableStateOf<String?>(null)
    var toc by mutableStateOf<List<TocItem>>(emptyList())
    var reflowable by mutableStateOf(false)
    var prefs by mutableStateOf(ReaderPrefs())
    var showHint by mutableStateOf(false)
    var sheet by mutableStateOf<Sheet?>(null)

    enum class Sheet { Toc, Text }
}

interface ReaderActions {
    fun close()
    fun goToProgression(progression: Double)
    fun goTo(item: TocItem)
    fun changePrefs(prefs: ReaderPrefs)
    fun dismissHint()
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
            TocSheet(ui) {
                ui.sheet = null
                actions.goTo(it)
            }
        }
        ReaderUi.Sheet.Text -> ModalBottomSheet(onDismissRequest = { ui.sheet = null }, containerColor = shelf.pane) {
            TextSheet(ui.prefs, actions::changePrefs)
        }
        null -> {}
    }
}

@Composable
private fun TopBar(ui: ReaderUi, actions: ReaderActions) {
    Column(Modifier.fillMaxWidth().background(shelf.pane).statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = actions::close) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.close_book), tint = shelf.fg)
            }
            Column(Modifier.weight(1f).padding(horizontal = 4.dp)) {
                Text(ui.title, color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                ui.chapter?.takeIf { it.isNotBlank() }?.let {
                    Text(it, color = shelf.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            if (ui.toc.isNotEmpty()) {
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
    Column(Modifier.fillMaxWidth().background(shelf.pane).navigationBarsPadding()) {
        HorizontalDivider(color = shelf.line)
        Column(Modifier.padding(horizontal = 20.dp, vertical = 10.dp)) {
            Slider(
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
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                val page = ui.page
                val pages = ui.pages
                Text(
                    if (!ui.reflowable && page != null && pages != null) stringResource(R.string.page_of, page, pages)
                    else ui.chapter.orEmpty(),
                    color = shelf.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(percent(shown.toDouble()), color = shelf.fg, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
            }
        }
    }
}

@Composable
private fun MarkHint(actions: ReaderActions) {
    Row(
        Modifier.navigationBarsPadding().padding(16.dp).fillMaxWidth().clip(RoundedCornerShape(8.dp))
            .background(shelf.fg).padding(start = 16.dp, top = 12.dp, bottom = 12.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.FormatQuote, null, tint = shelf.inverse, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(12.dp))
        Text(stringResource(R.string.mark_hint), color = shelf.inverse, fontSize = 13.sp, lineHeight = 18.sp, modifier = Modifier.weight(1f))
        IconButton(onClick = actions::dismissHint) {
            Icon(Icons.Outlined.Close, stringResource(R.string.got_it), tint = shelf.inverse)
        }
    }
}

@Composable
internal fun TocSheet(ui: ReaderUi, onPick: (TocItem) -> Unit) {
    val current = ui.toc.indexOfLast { it.href.substringBefore('#') == ui.currentHref?.substringBefore('#') }
    val list = rememberLazyListState()
    LaunchedEffect(Unit) { if (current > 2) list.scrollToItem(current - 2) }
    Column(Modifier.padding(bottom = 16.dp)) {
        Text(
            stringResource(R.string.contents), color = shelf.fg, fontSize = 18.sp, fontWeight = FontWeight.Bold,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
        )
        LazyColumn(state = list) {
            itemsIndexed(ui.toc) { index, item ->
                val selected = index == current
                Row(
                    Modifier.fillMaxWidth().clickable { onPick(item) }
                        .background(if (selected) shelf.soft else shelf.pane)
                        .padding(start = (20 + item.depth * 16).dp, end = 20.dp, top = 14.dp, bottom = 14.dp),
                ) {
                    Text(
                        item.title, color = shelf.fg, fontSize = 15.sp,
                        fontWeight = if (selected || item.depth == 0) FontWeight.SemiBold else FontWeight.Normal,
                    )
                }
            }
        }
    }
}

@Composable
internal fun TextSheet(prefs: ReaderPrefs, onChange: (ReaderPrefs) -> Unit) {
    Column(Modifier.padding(start = 20.dp, end = 20.dp, bottom = 28.dp)) {
        SectionTitle(stringResource(R.string.text_size))
        Row(verticalAlignment = Alignment.CenterVertically) {
            SizeButton("A−", stringResource(R.string.text_smaller)) { onChange(prefs.copy(fontSize = (prefs.fontSize - 0.1).coerceAtLeast(0.6))) }
            Text(
                percent(prefs.fontSize).replace(" ", ""), color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f), textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
            SizeButton("A+", stringResource(R.string.text_larger)) { onChange(prefs.copy(fontSize = (prefs.fontSize + 0.1).coerceAtMost(2.5))) }
        }
        Spacer(Modifier.height(16.dp))
        SectionTitle(stringResource(R.string.theme))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(R.string.theme_light, R.string.theme_sepia, R.string.theme_dark).forEachIndexed { i, label ->
                Pill(stringResource(label), prefs.theme == i) { onChange(prefs.copy(theme = i)) }
            }
        }
        Spacer(Modifier.height(16.dp))
        SectionTitle(stringResource(R.string.font))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(R.string.font_book, R.string.font_serif, R.string.font_sans).forEachIndexed { i, label ->
                Pill(stringResource(label), prefs.font == i) { onChange(prefs.copy(font = i)) }
            }
        }
        Spacer(Modifier.height(16.dp))
        SectionTitle(stringResource(R.string.line_spacing))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(R.string.spacing_compact, R.string.spacing_normal, R.string.spacing_wide).forEachIndexed { i, label ->
                Pill(stringResource(label), prefs.spacing == i) { onChange(prefs.copy(spacing = i)) }
            }
        }
        Spacer(Modifier.height(16.dp))
        Row(
            Modifier.fillMaxWidth().clickable { onChange(prefs.copy(scroll = !prefs.scroll)) }.padding(vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.scroll_mode), color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                Text(stringResource(R.string.scroll_mode_help), color = shelf.muted, fontSize = 12.sp)
            }
            Switch(
                checked = prefs.scroll, onCheckedChange = { onChange(prefs.copy(scroll = it)) },
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
        Modifier.size(width = 64.dp, height = 44.dp).clip(RoundedCornerShape(6.dp)).background(shelf.soft)
            .clickable(onClickLabel = description, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Text(label, color = shelf.fg, fontSize = 16.sp, fontWeight = FontWeight.Bold) }
}


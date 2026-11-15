package io.github.danielpereod.opdshelf.ui

import androidx.compose.animation.core.animateFloat
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowRight
import androidx.compose.material.icons.outlined.BarChart
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.data.LocalBook
import io.github.danielpereod.opdshelf.data.ReadingStats
import io.github.danielpereod.opdshelf.data.readingTime
import java.time.LocalDate
import java.time.YearMonth
import java.time.format.DateTimeFormatter
import java.util.Locale

/** "40 min", "2 h 10 min", "13 h": how long was read, as the web panel writes it. */
@Composable
fun durationText(seconds: Long): String {
    val time = readingTime(seconds)
    return when {
        time.hours == 0L -> stringResource(R.string.duration_minutes, time.minutes)
        time.minutes == 0L -> stringResource(R.string.duration_hours, time.hours)
        else -> stringResource(R.string.duration_hours_minutes, time.hours, time.minutes)
    }
}

@Composable
private fun locale(): Locale = LocalConfiguration.current.locales[0] ?: Locale.getDefault()

/**
 * The line on My books that leads to the statistics: "Today 40 min · Streak of 12 days",
 * or "No reading yet today" on a server with no reading yet, so the statistics
 * are always one tap away. Nothing until the server answered once.
 */
@Composable
fun StatsStrip(state: StatsState, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val stats = state.stats ?: return
    // Offline with yesterday's numbers, "today" would be wrong: the strip only says when they are from
    val fresh = remember(state.fetchedAt) {
        java.time.Instant.ofEpochMilli(state.fetchedAt).atZone(java.time.ZoneId.systemDefault()).toLocalDate() == LocalDate.now()
    }
    val text = if (fresh && stats.today.seconds <= 0 && stats.streak.current <= 0) {
        stringResource(R.string.stats_no_reading_today)
    } else if (fresh) listOfNotNull(
        stringResource(R.string.stats_today_short, durationText(stats.today.seconds)),
        stats.streak.current.takeIf { it > 0 }?.let { pluralStringResource(R.plurals.stats_streak_short, it, it) },
    ).joinToString(" · ") else stringResource(R.string.reading_stats)
    val detail = if (fresh) stringResource(R.string.reading_stats) else stringResource(R.string.stats_updated, relativeTime(state.fetchedAt))
    Row(
        modifier.fillMaxWidth().heightIn(min = 56.dp).clip(RoundedCornerShape(8.dp)).background(shelf.pane)
            .border(1.dp, shelf.line, RoundedCornerShape(8.dp))
            .clickable(role = Role.Button, onClickLabel = stringResource(R.string.reading_stats), onClick = onOpen)
            .padding(start = Space.l, end = Space.s, top = Space.s, bottom = Space.s),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.m),
    ) {
        Icon(Icons.Outlined.BarChart, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
        Column(Modifier.weight(1f)) {
            Text(text, color = shelf.fg, fontSize = TextSize.bodySmall, fontWeight = FontWeight.SemiBold, lineHeight = 20.sp)
            Text(detail, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
        }
        Icon(Icons.AutoMirrored.Outlined.KeyboardArrowRight, null, tint = shelf.muted)
    }
}

/** Reading statistics from every device, as the web panel's Statistics page shows them. */
@Composable
fun StatsScreen(library: LibraryViewModel, onBack: () -> Unit, showBook: (LocalBook) -> Unit, modifier: Modifier) {
    val state by library.stats.collectAsState()
    val books by library.books.collectAsState()
    LaunchedEffect(Unit) { library.loadStats() }
    val stats = state.stats

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(
            stringResource(R.string.reading_stats), onBack = onBack,
            actions = {
                if (state.loading && stats != null) {
                    Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) { Spinner(size = 20.dp) }
                } else if (stats != null) {
                    IconButton(onClick = { library.loadStats() }) {
                        Icon(Icons.Outlined.Refresh, stringResource(R.string.refresh), tint = shelf.fg)
                    }
                }
            },
        )
        when {
            stats == null && state.error != null -> EmptyState(
                icon = Icons.Outlined.CloudOff,
                title = stringResource(R.string.stats_cannot_load),
                text = state.error.orEmpty(),
                action = stringResource(R.string.retry),
                onAction = { library.loadStats() },
            )
            stats == null -> StatsSkeleton()
            stats.isEmpty -> EmptyState(
                icon = Icons.Outlined.BarChart,
                title = stringResource(R.string.stats_empty_title),
                text = stringResource(R.string.stats_empty_text),
            )
            else -> StatsContent(stats, state, books, showBook, onRetry = { library.loadStats() })
        }
    }
}

@Composable
private fun StatsContent(
    stats: ReadingStats,
    state: StatsState,
    books: List<LocalBook>,
    showBook: (LocalBook) -> Unit,
    onRetry: () -> Unit,
) {
    val locale = locale()
    val year = remember { LocalDate.now().year.toString() }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(Modifier.widthIn(max = MaxContentWidth).fillMaxWidth().padding(horizontal = edgePadding, vertical = Space.l)) {
            // Shown offline from the last time, and says from when
            if (state.error != null) {
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(shelf.soft)
                        .padding(start = Space.l, end = Space.xs, top = Space.xs, bottom = Space.xs)
                        .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Outlined.CloudOff, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(Space.m))
                    Text(
                        stringResource(R.string.stats_offline, relativeTime(state.fetchedAt)),
                        color = shelf.fg, fontSize = TextSize.label, lineHeight = 18.sp,
                        modifier = Modifier.weight(1f).padding(vertical = Space.s),
                    )
                    androidx.compose.material3.TextButton(onClick = onRetry, modifier = Modifier.heightIn(min = 48.dp)) {
                        Text(stringResource(R.string.retry), color = shelf.fg, fontSize = TextSize.label, fontWeight = FontWeight.SemiBold)
                    }
                }
                Spacer(Modifier.height(Space.l))
            }

            val tiles = listOf(
                Triple(
                    stringResource(R.string.stats_today), durationText(stats.today.seconds),
                    pluralStringResource(R.plurals.stats_sessions, stats.today.sessions, stats.today.sessions),
                ),
                Triple(
                    stringResource(R.string.stats_streak),
                    pluralStringResource(R.plurals.stats_days, stats.streak.current, stats.streak.current),
                    stringResource(R.string.stats_longest, pluralStringResource(R.plurals.stats_days, stats.streak.longest, stats.streak.longest)),
                ),
                Triple(
                    stringResource(R.string.stats_last_30), durationText(stats.last30Seconds),
                    pluralStringResource(R.plurals.stats_reading_days, stats.readingDaysIn30, stats.readingDaysIn30),
                ),
                Triple(
                    stringResource(R.string.stats_finished_year), stats.finishedIn(year).toString(),
                    stringResource(R.string.stats_in_year, year),
                ),
            )
            // As many tiles in a row as fit at the font's size (4, 2 or 1, never 3 and 1),
            // so no label has to break inside a word; the tiles of a row are as tall as each other
            BoxWithConstraints(Modifier.fillMaxWidth()) {
                val minTile = 150.dp * LocalDensity.current.fontScale
                val fit = ((maxWidth + Space.m) / (minTile + Space.m)).toInt()
                val perRow = when {
                    fit >= 4 -> 4
                    fit >= 2 -> 2
                    else -> 1
                }
                Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
                    tiles.chunked(perRow).forEach { row ->
                        Row(
                            horizontalArrangement = Arrangement.spacedBy(Space.m),
                            modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Min),
                        ) {
                            row.forEach { (label, value, detail) -> StatTile(label, value, detail, Modifier.weight(1f).fillMaxHeight()) }
                            repeat(perRow - row.size) { Spacer(Modifier.weight(1f)) }
                        }
                    }
                }
            }

            SectionTitle(stringResource(R.string.stats_per_day), Modifier.padding(top = Space.xxl))
            val dayFull = remember(locale) { DateTimeFormatter.ofPattern("EEE d MMM", locale) }
            val dayShort = remember(locale) { DateTimeFormatter.ofPattern("d MMM", locale) }
            val days = stats.days.mapNotNull { d -> runCatching { LocalDate.parse(d.date) to d.seconds }.getOrNull() }
            val dayDescriptions = days.map { (date, seconds) ->
                stringResource(R.string.stats_bar, date.format(dayFull), durationText(seconds))
            }
            val maxDay = days.maxOfOrNull { it.second } ?: 0
            BarChart(
                values = days.map { it.second },
                descriptions = dayDescriptions,
                maxLabel = durationText(maxDay),
                axis = axisLabels(days.map { it.first.format(dayShort) }),
            )

            SectionTitle(stringResource(R.string.stats_per_month), Modifier.padding(top = Space.xxl))
            val monthFull = remember(locale) { DateTimeFormatter.ofPattern("LLLL yyyy", locale) }
            val monthShort = remember(locale) { DateTimeFormatter.ofPattern("LLL", locale) }
            val months = stats.finished.mapNotNull { m -> runCatching { YearMonth.parse(m.month) to m.count }.getOrNull() }
            val maxMonth = months.maxOfOrNull { it.second } ?: 0
            BarChart(
                values = months.map { it.second.toLong() },
                descriptions = months.map { (month, count) ->
                    stringResource(R.string.stats_bar, month.format(monthFull), pluralStringResource(R.plurals.book_count, count, count))
                },
                maxLabel = pluralStringResource(R.plurals.book_count, maxMonth, maxMonth),
                axis = axisLabels(months.map { it.first.format(monthShort) }),
                // A flat line with "0 books" says nothing: a sentence instead
                empty = stringResource(R.string.stats_finished_empty),
            )

            if (stats.topBooks.isNotEmpty()) {
                SectionTitle(stringResource(R.string.stats_top_books), Modifier.padding(top = Space.xxl))
                Column(Modifier.clip(RoundedCornerShape(8.dp)).background(shelf.pane).border(1.dp, shelf.line, RoundedCornerShape(8.dp))) {
                    stats.topBooks.forEachIndexed { i, top ->
                        if (i > 0) HorizontalDivider(color = shelf.line)
                        // On this device, it opens its details
                        val local = books.firstOrNull { it.document == top.document }
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 56.dp)
                                .then(if (local != null) Modifier.clickable(onClickLabel = stringResource(R.string.details)) { showBook(local) } else Modifier)
                                .padding(horizontal = Space.l, vertical = Space.s)
                                .semantics(mergeDescendants = true) {},
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text("${i + 1}", color = shelf.muted, fontSize = TextSize.label, fontWeight = FontWeight.SemiBold, modifier = Modifier.width(28.dp))
                            Text(
                                top.title ?: local?.title ?: stringResource(R.string.unknown_book),
                                color = shelf.fg, fontSize = TextSize.bodySmall, fontWeight = FontWeight.SemiBold, lineHeight = 20.sp,
                                maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                            )
                            Spacer(Modifier.width(Space.m))
                            Text(durationText(top.seconds), color = shelf.muted, fontSize = TextSize.label, lineHeight = 18.sp)
                            if (local != null) Icon(Icons.AutoMirrored.Outlined.KeyboardArrowRight, null, tint = shelf.muted, modifier = Modifier.padding(start = Space.xs))
                        }
                    }
                }
            }

            Spacer(Modifier.height(Space.xl))
            Text(
                stringResource(R.string.stats_explained), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp,
            )
            if (state.error == null && state.fetchedAt > 0) {
                Spacer(Modifier.height(Space.s))
                Text(
                    stringResource(R.string.stats_updated, relativeTime(state.fetchedAt)),
                    color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp,
                )
            }
            Spacer(Modifier.height(Space.xl))
        }
    }
}

/** The first, middle and last labels: enough to read the dates without crowding them. */
private fun axisLabels(all: List<String>): List<String> =
    if (all.size <= 3) all else listOf(all.first(), all[all.size / 2], all.last())

@Composable
private fun StatTile(label: String, value: String, detail: String, modifier: Modifier = Modifier) {
    Column(
        modifier.clip(RoundedCornerShape(8.dp)).background(shelf.pane).border(1.dp, shelf.line, RoundedCornerShape(8.dp))
            .padding(Space.l).semantics(mergeDescendants = true) {},
    ) {
        Text(
            label.uppercase(), color = shelf.muted, fontSize = TextSize.overline, fontWeight = FontWeight.Bold,
            letterSpacing = 0.8.sp, lineHeight = 16.sp, maxLines = 2, overflow = TextOverflow.Ellipsis,
        )
        Spacer(Modifier.height(Space.xs))
        Text(value, color = shelf.fg, fontSize = TextSize.headline, fontWeight = FontWeight.Bold, lineHeight = 28.sp)
        Text(detail, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * Bars over time, drawn with plain boxes. Each bar reads out its own date and
 * value; the highest value sits at the top line and a few dates under it give the scale.
 */
@Composable
private fun BarChart(values: List<Long>, descriptions: List<String>, maxLabel: String, axis: List<String>, empty: String? = null) {
    val max = values.maxOrNull()?.coerceAtLeast(1) ?: 1
    val box = Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(shelf.pane).border(1.dp, shelf.line, RoundedCornerShape(8.dp)).padding(Space.l)
    if (empty != null && values.all { it <= 0 }) {
        Text(empty, color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp, modifier = box)
        return
    }
    Column(box) {
        // Each bar says its own value, so the scale is for the eyes only
        Text(maxLabel, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, modifier = Modifier.clearAndSetSemantics {})
        HorizontalDivider(color = shelf.line, modifier = Modifier.padding(top = Space.xs))
        Row(
            Modifier.fillMaxWidth().height(120.dp),
            horizontalArrangement = Arrangement.spacedBy(2.dp),
            verticalAlignment = Alignment.Bottom,
        ) {
            values.forEachIndexed { i, value ->
                val description = descriptions.getOrElse(i) { "" }
                BoxWithConstraints(Modifier.weight(1f).fillMaxHeight().semantics { contentDescription = description }) {
                    val height = if (value > 0) maxOf(maxHeight * (value.toFloat() / max), 2.dp) else 2.dp
                    // A bar no wider than a bar should be, standing on the baseline even when it is the only one
                    Box(
                        Modifier.align(Alignment.BottomCenter).fillMaxWidth().widthIn(max = 28.dp).height(height)
                            .clip(RoundedCornerShape(topStart = 2.dp, topEnd = 2.dp))
                            .background(if (value > 0) shelf.fg else shelf.lineStrong.copy(alpha = 0.4f)),
                    )
                }
            }
        }
        // The baseline the bars stand on, with the scale's zero at its start
        HorizontalDivider(thickness = 1.5.dp, color = shelf.muted)
        Row(Modifier.fillMaxWidth().padding(top = Space.xs), horizontalArrangement = Arrangement.SpaceBetween) {
            val labels = if (axis.size == 1) axis + "" else axis
            labels.forEach {
                Text(it, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 1, modifier = Modifier.clearAndSetSemantics {})
            }
        }
    }
}

/** Grey shapes where the numbers will be, while the server answers for the first time. */
@Composable
private fun StatsSkeleton() {
    val pulse = androidx.compose.animation.core.rememberInfiniteTransition(label = "skeleton")
    val alpha by pulse.animateFloat(
        0.5f, 1f,
        androidx.compose.animation.core.infiniteRepeatable(androidx.compose.animation.core.tween<Float>(800), androidx.compose.animation.core.RepeatMode.Reverse),
        label = "alpha",
    )
    val loading = stringResource(R.string.loading)
    val block = Modifier.graphicsLayer { this.alpha = alpha }.clip(RoundedCornerShape(8.dp)).background(shelf.line)
    Column(
        Modifier.fillMaxSize().padding(horizontal = edgePadding, vertical = Space.l).semantics { contentDescription = loading },
        verticalArrangement = Arrangement.spacedBy(Space.m),
    ) {
        repeat(2) {
            Row(horizontalArrangement = Arrangement.spacedBy(Space.m)) {
                Box(block.weight(1f).height(96.dp))
                Box(block.weight(1f).height(96.dp))
            }
        }
        Spacer(Modifier.height(Space.l))
        Box(block.fillMaxWidth().height(180.dp))
    }
}

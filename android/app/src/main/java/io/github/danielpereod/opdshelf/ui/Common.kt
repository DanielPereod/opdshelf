package io.github.danielpereod.opdshelf.ui

import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.CloudDone
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.CloudUpload
import androidx.compose.material.icons.outlined.DownloadDone
import androidx.compose.material.icons.outlined.DoneAll
import androidx.compose.material.icons.outlined.Sync
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.ImageLoader
import coil.compose.AsyncImage
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.sync.SyncStatus
import java.util.Locale

@Composable
fun rememberImageLoader(): ImageLoader {
    val context = LocalContext.current
    return remember { ImageLoader.Builder(context).okHttpClient(context.app.api.client).crossfade(android.os.Build.FINGERPRINT != "robolectric").build() }
}

/** A cover, or a soft tile with the title on it when there is none. Covers share one small radius. */
@Composable
fun Cover(model: Any?, title: String, label: String?, loader: ImageLoader, modifier: Modifier, large: Boolean = false) {
    val shape = RoundedCornerShape(4.dp)
    BoxWithConstraints(
        modifier.clip(shape).background(shelf.soft).border(1.dp, shelf.line, shape),
        contentAlignment = Alignment.Center,
    ) {
        var loaded by remember(model) { mutableStateOf(false) }
        val text = placeholderText(maxWidth.value, large)
        if (!loaded) Column(Modifier.padding(if (large) Space.m else Space.xs), horizontalAlignment = Alignment.CenterHorizontally) {
            if (text.titleSp != null) {
                Text(
                    title, color = shelf.fg, fontSize = text.titleSp.sp, lineHeight = text.titleLineSp.sp, fontWeight = FontWeight.SemiBold,
                    textAlign = TextAlign.Center, maxLines = text.titleLines, overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(if (large) Space.s else Space.xs))
            }
            Text(
                (label ?: stringResource(R.string.cover_book)).uppercase(), color = shelf.muted,
                fontSize = text.tagSp.sp,
                fontWeight = FontWeight.Bold, letterSpacing = 0.5.sp, maxLines = 1,
            )
        }
        if (model != null) {
            AsyncImage(
                model = model, imageLoader = loader, contentDescription = null,
                contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize(),
                onSuccess = { loaded = true },
            )
        }
    }
}

/** What a cover without a picture says: its title in [titleLines] lines (none when null) and the format as a small tag. */
data class PlaceholderText(val titleSp: Int?, val titleLineSp: Int, val titleLines: Int, val tagSp: Int)

/**
 * The placeholder's text for a tile [widthDp] wide: the title as large tiles
 * show it, three readable lines on a grid's tile, and only the format on a
 * list's small cover, whose row has the title beside it.
 */
fun placeholderText(widthDp: Float, large: Boolean): PlaceholderText = when {
    large || widthDp >= 120f -> PlaceholderText(titleSp = 15, titleLineSp = 20, titleLines = 4, tagSp = 11)
    widthDp >= 64f -> PlaceholderText(titleSp = 12, titleLineSp = 16, titleLines = 3, tagSp = 10)
    else -> PlaceholderText(titleSp = null, titleLineSp = 0, titleLines = 0, tagSp = 10)
}

/** Whether every one of [visible] is picked. */
fun allPicked(picked: List<String>, visible: List<String>): Boolean =
    visible.isNotEmpty() && visible.all { it in picked }

/** Select all of [visible], or deselect them all when they are all picked already; picks not shown stay. */
fun toggleAll(picked: List<String>, visible: List<String>): List<String> =
    if (allPicked(picked, visible)) picked - visible.toSet() else (picked + visible).distinct()

fun formatSize(bytes: Long?): String? {
    if (bytes == null || bytes <= 0) return null
    val units = listOf("B", "KB", "MB", "GB")
    var value = bytes.toDouble()
    var unit = 0
    while (value >= 1024 && unit < units.lastIndex) {
        value /= 1024
        unit++
    }
    return if (unit == 0) "$bytes B" else String.format(Locale.getDefault(), "%.1f %s", value, units[unit])
}

/** How long ago a time was, in the unit it is said in. */
sealed interface Age {
    data object JustNow : Age
    data class Minutes(val n: Int) : Age
    data class Hours(val n: Int) : Age
    data class Days(val n: Int) : Age
    /** A week or more: said as a date. */
    data object Older : Age
}

/** [millis] against [now]: under a minute (or in the future, from a clock ahead) is just now. */
fun ageOf(millis: Long, now: Long): Age {
    val seconds = (now - millis) / 1000
    return when {
        seconds < 60 -> Age.JustNow
        seconds < 3600 -> Age.Minutes((seconds / 60).toInt())
        seconds < 86400 -> Age.Hours((seconds / 3600).toInt())
        seconds < 7 * 86400 -> Age.Days((seconds / 86400).toInt())
        else -> Age.Older
    }
}

/**
 * "just now", "5 minutes ago", "3 days ago", or a date, in the device's
 * language and in lower case, as it goes in the middle of a sentence
 * ("Changes synced 4 minutes ago"); never "0 minutes ago".
 */
@Composable
fun relativeTime(millis: Long): String = when (val age = ageOf(millis, System.currentTimeMillis())) {
    Age.JustNow -> stringResource(R.string.time_just_now)
    is Age.Minutes -> pluralStringResource(R.plurals.time_minutes_ago, age.n, age.n)
    is Age.Hours -> pluralStringResource(R.plurals.time_hours_ago, age.n, age.n)
    is Age.Days -> pluralStringResource(R.plurals.time_days_ago, age.n, age.n)
    Age.Older -> DateUtils.formatDateTime(
        androidx.compose.ui.platform.LocalContext.current, millis,
        DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_ABBREV_MONTH,
    )
}

/** "42 %" or "42%", as the device's language writes percentages. */
fun percent(fraction: Double): String =
    java.text.NumberFormat.getPercentInstance(Locale.getDefault()).format(fraction.coerceIn(0.0, 1.0))

/** A book of a grid: cover with the reading bar under it, title and author. */
@Composable
fun BookTile(
    title: String,
    authors: List<String>,
    cover: Any?,
    format: String?,
    loader: ImageLoader,
    onClick: () -> Unit,
    progress: Double? = null,
    downloaded: Boolean = false,
    downloading: Float? = null,
    onLongClick: (() -> Unit)? = null,
    menu: List<MenuAction> = emptyList(),
    finished: Boolean = false,
    /** Null outside selection mode; otherwise whether this tile is picked. */
    selected: Boolean? = null,
    longClickLabel: String? = null,
) {
    Column(
        Modifier.clip(RoundedCornerShape(6.dp))
            .combinedClickable(
                role = if (selected != null) Role.Checkbox else Role.Button, onClick = onClick, onLongClick = onLongClick,
                onLongClickLabel = onLongClick?.let { longClickLabel ?: stringResource(R.string.details) },
            )
            .semantics(mergeDescendants = true) { if (selected != null) this.selected = selected },
    ) {
        Box {
            val scale by androidx.compose.animation.core.animateFloatAsState(if (selected == true) 0.92f else 1f, label = "pick")
            Cover(
                cover, title, format, loader,
                Modifier.fillMaxWidth().aspectRatio(2f / 3f).graphicsLayer { scaleX = scale; scaleY = scale }
                    .then(if (selected == true) Modifier.border(3.dp, shelf.fg, RoundedCornerShape(6.dp)) else Modifier),
            )
            if (selected != null) {
                Box(
                    Modifier.align(Alignment.TopStart).padding(Space.s).size(28.dp).clip(RoundedCornerShape(14.dp))
                        .background(if (selected) shelf.fg else shelf.bg.copy(alpha = 0.85f))
                        .border(2.dp, shelf.fg, RoundedCornerShape(14.dp)),
                    contentAlignment = Alignment.Center,
                ) {
                    if (selected) Icon(Icons.Outlined.Check, null, tint = shelf.inverse, modifier = Modifier.size(18.dp))
                }
            }
            if (downloaded || downloading != null) {
                val description = stringResource(R.string.downloaded_mark)
                Box(
                    Modifier.align(Alignment.BottomEnd).padding(Space.s).size(24.dp).clip(RoundedCornerShape(12.dp))
                        .background(shelf.fg).semantics { contentDescription = description },
                    contentAlignment = Alignment.Center,
                ) {
                    if (downloading != null) Spinner(size = 14.dp, onDark = true)
                    else Icon(Icons.Outlined.DownloadDone, null, tint = shelf.inverse, modifier = Modifier.size(14.dp))
                }
            }
            if (menu.isNotEmpty() && selected == null) {
                MoreMenu(stringResource(R.string.more_options, title), menu, Modifier.align(Alignment.TopEnd), onImage = true)
            }
        }
        when {
            downloading != null -> ReadBar(downloading, Modifier.fillMaxWidth().padding(top = Space.s))
            finished -> ReadBar(1f, Modifier.fillMaxWidth().padding(top = Space.s))
            progress != null -> ReadBar(progress.toFloat(), Modifier.fillMaxWidth().padding(top = Space.s))
        }
        Spacer(Modifier.height(Space.s))
        Text(title, color = shelf.fg, fontSize = TextSize.bodySmall, fontWeight = FontWeight.SemiBold, maxLines = 2, overflow = TextOverflow.Ellipsis, lineHeight = 20.sp)
        if (authors.isNotEmpty()) {
            Text(authors.joinToString(", "), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (finished) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.DoneAll, null, tint = shelf.muted, modifier = Modifier.size(14.dp))
                Spacer(Modifier.width(Space.xs))
                Text(stringResource(R.string.finished), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
            }
        } else if (progress != null && progress > 0) {
            Text(percent(progress), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
        }
    }
}

/** One line saying whether reading positions reached the server, with a retry. */
@Composable
fun SyncLine(status: SyncStatus, onSync: () -> Unit, modifier: Modifier = Modifier) {
    val (icon, text) = when {
        status.syncing -> Icons.Outlined.CloudUpload to stringResource(R.string.sync_sending)
        status.pending > 0 && status.failing ->
            Icons.Outlined.CloudOff to pluralStringResource(R.plurals.sync_pending_offline, status.pending, status.pending)
        status.pending > 0 -> Icons.Outlined.CloudUpload to pluralStringResource(R.plurals.sync_pending, status.pending, status.pending)
        status.lastSync > 0 -> Icons.Outlined.CloudDone to stringResource(R.string.sync_done, relativeTime(status.lastSync))
        else -> Icons.Outlined.CloudDone to stringResource(R.string.sync_ready)
    }
    Row(
        modifier.fillMaxWidth().heightIn(min = 56.dp).clip(RoundedCornerShape(8.dp)).background(shelf.soft)
            .padding(start = Space.l, end = Space.xs, top = Space.xs, bottom = Space.xs),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Space.m),
    ) {
        Icon(icon, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
        Text(text, color = shelf.fg, fontSize = TextSize.label, lineHeight = 18.sp, modifier = Modifier.weight(1f))
        SyncButton(status, onSync)
    }
}

/** Sync now, as an icon: a spinner while sending. Fits the header and grows with no text. */
@Composable
fun SyncButton(status: SyncStatus, onSync: () -> Unit) {
    if (status.syncing) {
        Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) { Spinner(size = 20.dp) }
    } else {
        androidx.compose.material3.IconButton(onClick = onSync) {
            Icon(
                if (status.pending > 0 && status.failing) Icons.Outlined.CloudOff else Icons.Outlined.Sync,
                stringResource(R.string.sync_now), tint = shelf.fg,
            )
        }
    }
}

@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier, trailing: @Composable () -> Unit = {}) {
    Row(modifier.fillMaxWidth().padding(top = Space.s, bottom = Space.m), verticalAlignment = Alignment.CenterVertically) {
        Text(
            text.uppercase(), color = shelf.muted, fontSize = TextSize.overline, fontWeight = FontWeight.Bold,
            letterSpacing = 0.8.sp, lineHeight = 16.sp, modifier = Modifier.weight(1f).semantics { heading() },
        )
        trailing()
    }
}

@Composable
fun Gap(size: Int) = Spacer(Modifier.width(size.dp).height(size.dp))

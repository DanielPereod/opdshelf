package io.github.danielpereod.opdshelf.ui

import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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

/** A cover, or a soft tile with the title on it when there is none. */
@Composable
fun Cover(model: Any?, title: String, label: String?, loader: ImageLoader, modifier: Modifier, large: Boolean = false) {
    Box(
        modifier.clip(RoundedCornerShape(3.dp)).background(shelf.soft).border(1.dp, shelf.line, RoundedCornerShape(3.dp)),
        contentAlignment = Alignment.Center,
    ) {
        var loaded by remember(model) { mutableStateOf(false) }
        if (!loaded) Column(Modifier.padding(if (large) 12.dp else 4.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            if (large) {
                Text(
                    title, color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                    textAlign = TextAlign.Center, maxLines = 4, overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(8.dp))
            }
            Text(
                (label ?: "BOOK").uppercase(), color = shelf.muted, fontSize = if (large) 10.sp else 8.sp,
                fontWeight = FontWeight.Bold, letterSpacing = 0.5.sp,
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

/** "5 min ago", "yesterday"… in the device's language. */
fun relativeTime(millis: Long): String =
    DateUtils.getRelativeTimeSpanString(millis, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString()

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
) {
    Column(
        Modifier.clip(RoundedCornerShape(6.dp))
            .combinedClickable(role = Role.Button, onClick = onClick, onLongClick = onLongClick, onLongClickLabel = onLongClick?.let { stringResource(R.string.details) })
            .semantics(mergeDescendants = true) {},
    ) {
        Box {
            Cover(cover, title, format, loader, Modifier.fillMaxWidth().aspectRatio(2f / 3f))
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
            if (menu.isNotEmpty()) {
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

package io.github.danielpereod.opdshelf.ui

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Devices
import androidx.compose.material.icons.outlined.FormatQuote
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.LocalBook
import java.io.File

/** A book's page: cover, summary, where it was left, and what can be done with it. */
@Composable
fun DetailScreen(library: LibraryViewModel, detail: Detail, openBook: (LocalBook) -> Unit, modifier: Modifier) {
    val books by library.books.collectAsState()
    val downloads by library.downloads.collectAsState()
    val remote by library.remote.collectAsState()
    val context = LocalContext.current
    val loader = rememberImageLoader()
    val entry = detail.entry
    val local = books.firstOrNull { it.id == detail.bookId }
    val downloading = downloads[detail.bookId]
    var confirmRemove by remember { mutableStateOf(false) }
    var expanded by remember { mutableStateOf(false) }

    val title = entry?.title ?: local?.title.orEmpty()
    val authors = entry?.authors ?: local?.authors.orEmpty()
    val summary = (entry?.summary ?: local?.summary)?.trim()?.takeIf { it.isNotEmpty() }
    val format = entry?.acquisition?.format ?: local?.format
    val size = entry?.acquisition?.length ?: local?.sizeBytes
    val cover: Any? = local?.coverFile?.let { File(context.app.store.booksDir, it) } ?: entry?.coverUrl ?: entry?.thumbnailUrl

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(stringResource(R.string.details), onBack = { library.closeDetail() })
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp)) {
            Row {
                Cover(cover, title, format, loader, Modifier.size(width = 120.dp, height = 180.dp), large = true)
                Spacer(Modifier.width(18.dp))
                Column(Modifier.weight(1f)) {
                    Text(title, color = shelf.fg, fontSize = 22.sp, fontWeight = FontWeight.Bold, lineHeight = 27.sp)
                    if (authors.isNotEmpty()) {
                        Spacer(Modifier.height(4.dp))
                        Text(authors.joinToString(", "), color = shelf.muted, fontSize = 15.sp)
                    }
                    Spacer(Modifier.height(12.dp))
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        format?.let { Badge(it) }
                        formatSize(size)?.let { Text(it, color = shelf.muted, fontSize = 12.sp, fontFamily = Mono) }
                    }
                    if (local != null) {
                        Spacer(Modifier.height(8.dp))
                        Text(stringResource(R.string.on_device), color = shelf.muted, fontSize = 12.sp)
                    }
                }
            }
            Spacer(Modifier.height(24.dp))

            // The main action, always in the same place
            when {
                local != null -> {
                    PrimaryButton(
                        if (local.percentage > 0) stringResource(R.string.continue_at, percent(local.percentage))
                        else stringResource(R.string.read),
                        { openBook(local) }, Modifier.fillMaxWidth(), height = 48.dp,
                    )
                    if (local.percentage > 0) {
                        Spacer(Modifier.height(10.dp))
                        ReadBar(local.percentage.toFloat(), Modifier.fillMaxWidth())
                    }
                }
                downloading != null -> {
                    PrimaryButton(
                        stringResource(R.string.downloading, percent(downloading.toDouble())), {},
                        Modifier.fillMaxWidth(), enabled = false, height = 48.dp,
                    )
                    Spacer(Modifier.height(10.dp))
                    ReadBar(downloading, Modifier.fillMaxWidth())
                }
                entry?.acquisition != null -> {
                    PrimaryButton(
                        listOfNotNull(stringResource(R.string.download), formatSize(size)).joinToString(" · "),
                        { library.download(entry) }, Modifier.fillMaxWidth(), height = 48.dp,
                    )
                    Spacer(Modifier.height(8.dp))
                    Text(stringResource(R.string.download_help), color = shelf.muted, fontSize = 12.sp, lineHeight = 17.sp)
                }
            }

            // Where it was left, as the server knows it
            remote?.takeIf { it.timestamp > 0 }?.let { position ->
                Spacer(Modifier.height(20.dp))
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(shelf.soft).padding(14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(if (position.manual) Icons.Outlined.FormatQuote else Icons.Outlined.Devices, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(12.dp))
                    Column(Modifier.weight(1f)) {
                        Text(
                            stringResource(if (position.manual) R.string.marked_on else R.string.left_on, position.device.ifEmpty { "?" }),
                            color = shelf.fg, fontSize = 14.sp, fontWeight = FontWeight.SemiBold,
                        )
                        Text(
                            "${percent(position.percentage)} · ${relativeTime(position.timestamp * 1000)}",
                            color = shelf.muted, fontSize = 12.sp,
                        )
                        position.context?.selected?.takeIf { position.manual && it.isNotBlank() }?.let {
                            Text("«$it»", color = shelf.muted, fontSize = 12.sp, maxLines = 2)
                        }
                    }
                }
            }

            summary?.let {
                Spacer(Modifier.height(24.dp))
                SectionTitle(stringResource(R.string.summary))
                Text(
                    it, color = shelf.fg, fontSize = 15.sp, lineHeight = 22.sp,
                    maxLines = if (expanded) Int.MAX_VALUE else 6,
                    modifier = Modifier.animateContentSize().clickable { expanded = !expanded },
                )
                if (!expanded && it.length > 280) {
                    Text(
                        stringResource(R.string.read_more), color = shelf.fg, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.padding(top = 6.dp).clickable { expanded = true },
                    )
                }
            }

            if (local != null) {
                Spacer(Modifier.height(32.dp))
                HorizontalDivider(color = shelf.line)
                Spacer(Modifier.height(16.dp))
                SecondaryButton(stringResource(R.string.remove_from_device), { confirmRemove = true }, Modifier.fillMaxWidth())
                Spacer(Modifier.height(6.dp))
                Text(stringResource(R.string.remove_help), color = shelf.muted, fontSize = 12.sp, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
            }
        }
    }

    if (confirmRemove && local != null) {
        AlertDialog(
            onDismissRequest = { confirmRemove = false },
            title = { Text(stringResource(R.string.remove_title)) },
            text = { Text(stringResource(R.string.remove_text, local.title)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmRemove = false
                    library.delete(local)
                    if (entry == null) library.closeDetail()
                }) { Text(stringResource(R.string.remove), color = shelf.fg, fontWeight = FontWeight.SemiBold) }
            },
            dismissButton = { TextButton(onClick = { confirmRemove = false }) { Text(stringResource(R.string.cancel), color = shelf.muted) } },
            containerColor = shelf.pane,
            shape = RoundedCornerShape(8.dp),
        )
    }
}

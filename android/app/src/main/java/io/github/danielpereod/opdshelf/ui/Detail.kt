package io.github.danielpereod.opdshelf.ui

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.DoneAll
import androidx.compose.material.icons.outlined.RemoveDone
import androidx.compose.material.icons.outlined.DownloadDone
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
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
    var cut by remember { mutableStateOf(false) }

    val title = entry?.title ?: local?.title.orEmpty()
    val authors = entry?.authors ?: local?.authors.orEmpty()
    val summary = (entry?.summary ?: local?.summary)?.trim()?.takeIf { it.isNotEmpty() }
    val format = entry?.acquisition?.format ?: local?.format
    val size = entry?.acquisition?.length ?: local?.sizeBytes
    val meta = entry?.meta?.takeUnless { it.isEmpty } ?: local?.meta
    val cover: Any? = local?.coverFile?.let { File(context.app.store.booksDir, it) } ?: entry?.coverUrl ?: entry?.thumbnailUrl

    val titleBlock: @Composable () -> Unit = {
        Text(
            title, color = shelf.fg, fontSize = TextSize.headline, fontWeight = FontWeight.Bold, lineHeight = 28.sp,
            modifier = Modifier.semantics { heading() },
        )
        if (authors.isNotEmpty()) {
            Spacer(Modifier.height(Space.xs))
            Text(authors.joinToString(", "), color = shelf.muted, fontSize = TextSize.body, lineHeight = 22.sp)
        }
        Spacer(Modifier.height(Space.m))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Space.s)) {
            format?.let { Badge(it) }
            formatSize(size)?.let { Text(it, color = shelf.muted, fontSize = TextSize.caption) }
        }
        if (local != null) {
            Spacer(Modifier.height(Space.s))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.DownloadDone, null, tint = shelf.muted, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(Space.xs))
                Text(stringResource(R.string.on_device), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
            }
        }
    }
    val mainAction: @Composable () -> Unit = {
        // The main action, always in the same place
        when {
            local != null -> {
                PrimaryButton(
                    if (local.percentage > 0) stringResource(R.string.continue_at, percent(local.percentage))
                    else stringResource(R.string.read),
                    { openBook(local) }, Modifier.fillMaxWidth(), height = 48.dp,
                )
                if (local.percentage > 0) {
                    Spacer(Modifier.height(Space.m))
                    ReadBar(if (local.finished) 1f else local.percentage.toFloat(), Modifier.fillMaxWidth())
                }
                Spacer(Modifier.height(Space.m))
                SecondaryButton(
                    stringResource(if (local.finished) R.string.mark_unfinished else R.string.mark_finished),
                    { library.setFinished(local, !local.finished) }, Modifier.fillMaxWidth(),
                    icon = if (local.finished) Icons.Outlined.RemoveDone else Icons.Outlined.DoneAll,
                )
            }
            downloading != null -> {
                PrimaryButton(
                    stringResource(R.string.downloading, percent(downloading.toDouble())), {},
                    Modifier.fillMaxWidth(), enabled = false, height = 48.dp,
                )
                Spacer(Modifier.height(Space.m))
                ReadBar(downloading, Modifier.fillMaxWidth())
                Spacer(Modifier.height(Space.m))
                SecondaryButton(stringResource(R.string.cancel_download), { library.cancelDownload(detail.bookId) }, Modifier.fillMaxWidth())
            }
            entry?.acquisition != null -> {
                PrimaryButton(
                    listOfNotNull(stringResource(R.string.download), formatSize(size)).joinToString(" · "),
                    { library.download(entry) }, Modifier.fillMaxWidth(), height = 48.dp,
                )
                Spacer(Modifier.height(Space.s))
                Text(stringResource(R.string.download_help), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
            }
        }
    }
    val more: @Composable () -> Unit = {
        // Where it was left, as the server knows it
        remote?.takeIf { it.timestamp > 0 }?.let { position ->
            Spacer(Modifier.height(Space.xl))
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(shelf.soft).padding(Space.l),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(if (position.manual) Icons.Outlined.FormatQuote else Icons.Outlined.Devices, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(Space.m))
                Column(Modifier.weight(1f)) {
                    Text(
                        stringResource(if (position.manual) R.string.marked_on else R.string.left_on, position.device.ifEmpty { "?" }),
                        color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp, fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        "${percent(position.percentage)} · ${relativeTime(position.timestamp * 1000)}",
                        color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp,
                    )
                    position.context?.selected?.takeIf { position.manual && it.isNotBlank() }?.let {
                        Text("«$it»", color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }

        summary?.let {
            Spacer(Modifier.height(Space.xxl))
            SectionTitle(stringResource(R.string.summary))
            Text(
                it, color = shelf.fg, fontSize = TextSize.body, lineHeight = 24.sp,
                maxLines = if (expanded) Int.MAX_VALUE else 6, overflow = TextOverflow.Ellipsis,
                onTextLayout = { layout -> if (!expanded) cut = layout.hasVisualOverflow },
                modifier = Modifier.animateContentSize(),
            )
            // Only when the summary really is cut: on a tablet six lines often hold all of it
            if (!expanded && cut) {
                Box(
                    Modifier.heightIn(min = 48.dp).clickable(role = Role.Button) { expanded = true },
                    contentAlignment = Alignment.CenterStart,
                ) {
                    Text(
                        stringResource(R.string.read_more), color = shelf.fg, fontSize = TextSize.bodySmall, fontWeight = FontWeight.SemiBold,
                        textDecoration = androidx.compose.ui.text.style.TextDecoration.Underline,
                    )
                }
            }
        }

        meta?.takeUnless { it.isEmpty }?.let { m ->
            Spacer(Modifier.height(Space.xxl))
            SectionTitle(stringResource(R.string.about_book))
            m.series?.let { s ->
                MetaRow(stringResource(R.string.meta_series), m.seriesIndex?.let { stringResource(R.string.meta_series_number, s, it) } ?: s)
            }
            m.publisher?.let { MetaRow(stringResource(R.string.meta_publisher), it) }
            m.issued?.let { MetaRow(stringResource(R.string.meta_issued), it.take(4).takeIf { y -> y.all(Char::isDigit) } ?: it) }
            m.language?.let { code ->
                val name = java.util.Locale.forLanguageTag(code).getDisplayLanguage(java.util.Locale.getDefault())
                    .replaceFirstChar { it.titlecase(java.util.Locale.getDefault()) }.ifEmpty { code }
                MetaRow(stringResource(R.string.meta_language), name)
            }
            if (m.subjects.isNotEmpty()) {
                Spacer(Modifier.height(Space.m))
                @OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
                androidx.compose.foundation.layout.FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(Space.s), verticalArrangement = Arrangement.spacedBy(Space.s),
                    modifier = Modifier.semantics(mergeDescendants = true) {},
                ) {
                    m.subjects.take(12).forEach { subject ->
                        Text(
                            subject, color = shelf.fg, fontSize = TextSize.caption, lineHeight = 16.sp,
                            modifier = Modifier.clip(RoundedCornerShape(12.dp)).background(shelf.soft)
                                .padding(horizontal = Space.m, vertical = 6.dp),
                        )
                    }
                }
            }
        }

        if (local != null) {
            Spacer(Modifier.height(Space.xxl))
            HorizontalDivider(color = shelf.line)
            Spacer(Modifier.height(Space.l))
            SecondaryButton(
                stringResource(R.string.remove_from_device), { confirmRemove = true }, Modifier.fillMaxWidth(),
                destructive = true, icon = Icons.Outlined.Delete,
            )
            Spacer(Modifier.height(Space.s))
            Text(stringResource(R.string.remove_help), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp, modifier = Modifier.fillMaxWidth())
        }
    }

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        // The title is right below, large: the bar only takes you back
        Header("", onBack = { library.closeDetail() })
        val twoPanes = LocalConfiguration.current.screenWidthDp >= 840
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()), horizontalAlignment = Alignment.CenterHorizontally) {
            if (twoPanes) {
                // Large tablets: cover and actions on the left, the book's words on the right
                Row(Modifier.widthIn(max = 1040.dp).fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.xl)) {
                    Column(Modifier.width(280.dp)) {
                        Cover(cover, title, format, loader, Modifier.fillMaxWidth().aspectRatio(2f / 3f), large = true)
                        Spacer(Modifier.height(Space.xl))
                        mainAction()
                    }
                    Spacer(Modifier.width(Space.xxl + Space.s))
                    Column(Modifier.weight(1f).widthIn(max = ReadableWidth)) {
                        titleBlock()
                        more()
                    }
                }
            } else {
                Column(Modifier.widthIn(max = ReadableWidth).fillMaxWidth().padding(horizontal = edgePadding, vertical = Space.l)) {
                    // With very large fonts the title goes under the cover instead of beside it
                    val stacked = androidx.compose.ui.platform.LocalDensity.current.fontScale >= 1.5f && !isWide
                    if (stacked) {
                        Cover(cover, title, format, loader, Modifier.size(width = 120.dp, height = 180.dp), large = true)
                        Spacer(Modifier.height(Space.l))
                        titleBlock()
                    } else Row {
                        val wide = isWide
                        Cover(
                            cover, title, format, loader,
                            Modifier.size(width = if (wide) 160.dp else 120.dp, height = if (wide) 240.dp else 180.dp), large = true,
                        )
                        Spacer(Modifier.width(if (wide) Space.xl else Space.l))
                        Column(Modifier.weight(1f)) { titleBlock() }
                    }
                    Spacer(Modifier.height(Space.xl))
                    mainAction()
                    more()
                }
            }
        }
    }

    if (confirmRemove && local != null) {
        ConfirmDialog(
            title = stringResource(R.string.remove_title),
            text = stringResource(R.string.remove_text, local.title),
            confirm = stringResource(R.string.remove),
            onConfirm = {
                confirmRemove = false
                library.delete(local)
                if (entry == null) library.closeDetail()
            },
            onDismiss = { confirmRemove = false },
        )
    }
}

@Composable
private fun MetaRow(label: String, value: String) {
    Column {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 44.dp).padding(vertical = Space.s).semantics(mergeDescendants = true) {},
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(label, color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp, modifier = Modifier.width(112.dp))
            Spacer(Modifier.width(Space.m))
            Text(value, color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp, modifier = Modifier.weight(1f))
        }
        HorizontalDivider(color = shelf.line)
    }
}

package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
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
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.R

/** The types offered when picking books to add; anything the server does not take is pointed out. */
val bookMimeTypes = arrayOf(
    "application/epub+zip", "application/pdf", "application/x-mobipocket-ebook", "application/vnd.amazon.ebook",
    "application/x-fictionbook+xml", "application/vnd.comicbook+zip", "application/vnd.comicbook-rar",
    "application/x-cbz", "application/x-cbr", "image/vnd.djvu", "application/rtf", "text/plain",
    "application/zip", "application/octet-stream",
)

/** Books picked on this device, the folder they go to on the server, and how they are getting on. */
@Composable
fun UploadDialog(upload: Upload, library: LibraryViewModel) {
    var choosingFolder by remember { mutableStateOf(false) }
    // Fetched as the sheet opens, so the folders are there when asked for
    androidx.compose.runtime.LaunchedEffect(Unit) { library.loadFolders() }
    val folders by library.folders.collectAsState()
    val supported = upload.files.count { it.supported && !it.done }
    val skipped = upload.files.count { !it.supported }
    val books = upload.files.count { it.supported }
    androidx.compose.material3.AlertDialog(
        onDismissRequest = { if (!upload.running) library.closeUpload() },
        title = {
            Text(pluralStringResource(R.plurals.upload_title, books, books), color = shelf.fg)
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
                // Where they go, changeable before starting
                Row(
                    Modifier.fillMaxWidth().heightIn(min = 48.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Outlined.Folder, null, tint = shelf.muted, modifier = Modifier.size(20.dp))
                    Spacer(Modifier.width(Space.m))
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(R.string.upload_folder), color = shelf.muted, fontSize = TextSize.caption)
                        Text(
                            upload.folder.ifEmpty { stringResource(R.string.library_top) },
                            color = shelf.fg, fontSize = TextSize.body, fontWeight = FontWeight.Medium,
                            maxLines = 2, overflow = TextOverflow.Ellipsis,
                        )
                    }
                    if (!upload.running && !upload.finished) {
                        TextButton(onClick = { choosingFolder = true }, modifier = Modifier.heightIn(min = 48.dp)) {
                            Text(stringResource(R.string.upload_change_folder), color = shelf.fg, fontWeight = FontWeight.SemiBold)
                        }
                    }
                }
                HorizontalDivider(color = shelf.line)
                LazyColumn(Modifier.heightIn(max = 280.dp)) {
                    items(upload.files, key = { it.uri.toString() }) { file -> UploadRow(file) }
                }
                if (skipped > 0) {
                    Text(
                        pluralStringResource(R.plurals.upload_skipped, skipped, skipped),
                        color = shelf.muted, fontSize = TextSize.label, lineHeight = 18.sp,
                    )
                }
            }
        },
        confirmButton = {
            when {
                upload.running -> TextButton(onClick = {}, enabled = false) {
                    Text(stringResource(R.string.uploading), color = shelf.muted)
                }
                supported > 0 -> TextButton(onClick = { library.startUpload() }) {
                    Text(
                        stringResource(if (upload.finished) R.string.retry else R.string.upload_start),
                        color = shelf.fg, fontWeight = FontWeight.SemiBold,
                    )
                }
                else -> {}
            }
        },
        dismissButton = {
            if (!upload.running) {
                TextButton(onClick = { library.closeUpload() }) {
                    Text(stringResource(if (upload.finished) R.string.close else R.string.cancel), color = shelf.fg)
                }
            }
        },
        containerColor = shelf.pane,
        shape = RoundedCornerShape(12.dp),
    )
    if (choosingFolder) {
        FolderPickerDialog(
            title = pluralStringResource(R.plurals.upload_title, books, books),
            folders = folders,
            current = upload.folder,
            heading = stringResource(R.string.upload_folder),
            onPick = {
                library.setUploadFolder(it)
                choosingFolder = false
            },
            onDismiss = { choosingFolder = false },
        )
    }
}

@Composable
private fun UploadRow(file: UploadFile) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = Space.s)
            .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val (icon, tint) = when {
            file.done -> Icons.Outlined.Check to shelf.fg
            file.error != null || !file.supported -> Icons.Outlined.ErrorOutline to shelf.danger
            else -> Icons.Outlined.Description to shelf.muted
        }
        Icon(icon, null, tint = tint, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(Space.m))
        Column(Modifier.weight(1f)) {
            Text(file.name, color = shelf.fg, fontSize = TextSize.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            val note = when {
                !file.supported -> stringResource(R.string.upload_unsupported)
                file.error != null -> file.error
                file.done -> stringResource(R.string.upload_done)
                file.size > 0 -> android.text.format.Formatter.formatShortFileSize(androidx.compose.ui.platform.LocalContext.current, file.size)
                else -> null
            }
            note?.let {
                Text(it, color = if (file.error != null || !file.supported) shelf.danger else shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
            }
            file.progress?.let { p ->
                Spacer(Modifier.size(Space.xs))
                LinearProgressIndicator(
                    progress = { p }, color = shelf.fg, trackColor = shelf.soft,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

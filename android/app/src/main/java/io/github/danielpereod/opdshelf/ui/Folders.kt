package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.data.ServerFolder

/**
 * Where to move a book: one tap on a folder (or the library's top level) moves it there,
 * and a new name creates the folder and moves it in. [folders] is null while they load.
 */
@Composable
fun FolderPickerDialog(
    title: String,
    folders: List<ServerFolder>?,
    current: String?,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
    heading: String = stringResource(R.string.move_to_folder),
) {
    var name by rememberSaveable { mutableStateOf("") }
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Column {
                Text(heading, color = shelf.fg)
                Text(title, color = shelf.muted, fontSize = TextSize.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(Space.l)) {
                Box(
                    Modifier.fillMaxWidth().heightIn(max = 280.dp).clip(RoundedCornerShape(8.dp))
                        .background(shelf.bg),
                ) {
                    if (folders == null) {
                        Box(Modifier.fillMaxWidth().padding(Space.xl), Alignment.Center) { Spinner() }
                    } else {
                        LazyColumn {
                            item {
                                FolderOption(stringResource(R.string.library_top), null, 0, Icons.Outlined.Home, current == "") { onPick("") }
                            }
                            items(folders, key = { it.path }) { folder ->
                                HorizontalDivider(color = shelf.line)
                                FolderOption(folder.name, folder.count, folder.depth, Icons.Outlined.Folder, current == folder.path) {
                                    onPick(folder.path)
                                }
                            }
                        }
                    }
                }
                LabeledField(
                    label = stringResource(R.string.or_new_folder),
                    value = name,
                    onChange = { name = it },
                    placeholder = stringResource(R.string.folder_name),
                    imeAction = ImeAction.Done,
                    onImeAction = { if (name.isNotBlank()) onPick(name.trim()) },
                    capitalization = KeyboardCapitalization.Sentences,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onPick(name.trim()) }, enabled = name.isNotBlank()) {
                Text(
                    stringResource(R.string.create_and_move),
                    color = if (name.isNotBlank()) shelf.fg else shelf.muted, fontWeight = FontWeight.SemiBold,
                )
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel), color = shelf.fg) }
        },
        containerColor = shelf.pane,
        shape = RoundedCornerShape(12.dp),
    )
}

@Composable
private fun FolderOption(
    label: String,
    count: Int?,
    depth: Int,
    icon: androidx.compose.ui.graphics.vector.ImageVector,
    here: Boolean,
    onClick: () -> Unit,
) {
    val color = if (here) shelf.muted else shelf.fg
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp)
            .clickable(enabled = !here, role = Role.Button, onClick = onClick)
            .padding(start = Space.m + 16.dp * depth, end = Space.m),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(icon, null, tint = shelf.muted, modifier = Modifier.size(20.dp))
        Spacer(Modifier.width(Space.m))
        Column(Modifier.weight(1f)) {
            Text(label, color = color, fontSize = TextSize.bodySmall, lineHeight = 20.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (here) Text(stringResource(R.string.already_here), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
        }
        if (count != null) Text(count.toString(), color = shelf.muted, fontSize = TextSize.caption)
    }
}

/** Asks for the name of a new folder. */
@Composable
fun NewFolderDialog(onCreate: (String) -> Unit, onDismiss: () -> Unit) {
    var name by rememberSaveable { mutableStateOf("") }
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.new_folder), color = shelf.fg) },
        text = {
            LabeledField(
                label = stringResource(R.string.folder_name),
                value = name,
                onChange = { name = it },
                imeAction = ImeAction.Done,
                onImeAction = { if (name.isNotBlank()) onCreate(name.trim()) },
                capitalization = KeyboardCapitalization.Sentences,
            )
        },
        confirmButton = {
            TextButton(onClick = { onCreate(name.trim()) }, enabled = name.isNotBlank()) {
                Text(stringResource(R.string.create), color = if (name.isNotBlank()) shelf.fg else shelf.muted, fontWeight = FontWeight.SemiBold)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel), color = shelf.fg) }
        },
        containerColor = shelf.pane,
        shape = RoundedCornerShape(12.dp),
    )
}

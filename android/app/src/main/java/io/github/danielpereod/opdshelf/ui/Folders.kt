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
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.pluralStringResource
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
 * Whether [name] can name one folder: not empty, and without the "/" or "\\" that would
 * make folders inside folders, nor "." or "..".
 */
fun isFolderName(name: String): Boolean {
    val trimmed = name.trim()
    return trimmed.isNotEmpty() && '/' !in trimmed && '\\' !in trimmed && trimmed != "." && trimmed != ".."
}

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
    val validName = isFolderName(name)
    // Said only once something is typed, not for an empty field
    val wrongName = name.isNotBlank() && !validName
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
                    help = if (wrongName) stringResource(R.string.folder_name_one_level) else null,
                    isError = wrongName,
                    imeAction = ImeAction.Done,
                    onImeAction = { if (validName) onPick(name.trim()) },
                    capitalization = KeyboardCapitalization.Sentences,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onPick(name.trim()) }, enabled = validName) {
                Text(
                    stringResource(R.string.create_and_move),
                    color = if (validName) shelf.fg else shelf.muted, fontWeight = FontWeight.SemiBold,
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
        if (count != null) Text(pluralStringResource(R.plurals.book_count, count, count), color = shelf.muted, fontSize = TextSize.caption)
    }
}

/** Asks for the name of a new folder. */
@Composable
fun NewFolderDialog(onCreate: (String) -> Unit, onDismiss: () -> Unit) = FolderNameDialog(
    title = stringResource(R.string.new_folder), confirm = stringResource(R.string.create),
    onDone = onCreate, onDismiss = onDismiss,
)

/** Asks for a folder's name: a new one, or a new name for one ([initial]). */
@Composable
fun FolderNameDialog(title: String, confirm: String, onDone: (String) -> Unit, onDismiss: () -> Unit, initial: String = "") {
    var name by rememberSaveable(stateSaver = androidx.compose.ui.text.input.TextFieldValue.Saver) {
        // The whole name selected, to type a new one straight away
        mutableStateOf(androidx.compose.ui.text.input.TextFieldValue(initial, androidx.compose.ui.text.TextRange(0, initial.length)))
    }
    val focus = androidx.compose.runtime.remember { androidx.compose.ui.focus.FocusRequester() }
    androidx.compose.runtime.LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    val valid = isFolderName(name.text)
    val wrong = name.text.isNotBlank() && !valid
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title, color = shelf.fg) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(Space.s)) {
                Text(stringResource(R.string.folder_name), color = shelf.fg, fontSize = TextSize.label, fontWeight = FontWeight.SemiBold)
                androidx.compose.material3.OutlinedTextField(
                    value = name, onValueChange = { name = it }, singleLine = true, isError = wrong,
                    shape = RoundedCornerShape(6.dp),
                    keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(
                        imeAction = ImeAction.Done, capitalization = KeyboardCapitalization.Sentences,
                    ),
                    keyboardActions = androidx.compose.foundation.text.KeyboardActions(onDone = { if (valid) onDone(name.text.trim()) }),
                    colors = androidx.compose.material3.OutlinedTextFieldDefaults.colors(
                        focusedBorderColor = shelf.fg, unfocusedBorderColor = shelf.lineStrong,
                        focusedContainerColor = shelf.pane, unfocusedContainerColor = shelf.pane,
                        cursorColor = shelf.fg, focusedTextColor = shelf.fg, unfocusedTextColor = shelf.fg,
                        errorBorderColor = shelf.danger, errorContainerColor = shelf.pane, errorCursorColor = shelf.fg,
                        errorTextColor = shelf.fg,
                    ),
                    modifier = Modifier.fillMaxWidth().focusRequester(focus),
                )
                if (wrong) {
                    Text(stringResource(R.string.folder_name_one_level), color = shelf.danger, fontSize = TextSize.caption, lineHeight = 16.sp)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onDone(name.text.trim()) }, enabled = valid) {
                Text(confirm, color = if (valid) shelf.fg else shelf.muted, fontWeight = FontWeight.SemiBold)
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel), color = shelf.fg) }
        },
        containerColor = shelf.pane,
        shape = RoundedCornerShape(12.dp),
    )
}

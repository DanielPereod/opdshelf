package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.EditNote
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.data.Annotation

internal val colorNames = mapOf(
    "yellow" to R.string.color_yellow, "green" to R.string.color_green, "blue" to R.string.color_blue,
    "red" to R.string.color_red, "purple" to R.string.color_purple, "orange" to R.string.color_orange,
    "cyan" to R.string.color_cyan, "olive" to R.string.color_olive, "gray" to R.string.color_gray,
)

fun highlightColor(name: String): Color = Color(Annotation.argb(name))

/** Round colour buttons, the chosen one ringed. */
@Composable
fun ColorPicker(selected: String, onPick: (String) -> Unit, modifier: Modifier = Modifier) {
    val colors = if (selected in Annotation.PICK) Annotation.PICK else Annotation.PICK + selected
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(Space.xs), verticalAlignment = Alignment.CenterVertically) {
        colors.forEach { c ->
            val name = stringResource(colorNames[c] ?: R.string.color_yellow)
            Box(
                Modifier.size(48.dp).clip(CircleShape)
                    .clickable(role = Role.RadioButton) { onPick(c) }
                    .semantics { contentDescription = name; this.selected = c == selected },
                contentAlignment = Alignment.Center,
            ) {
                Box(
                    Modifier.size(30.dp)
                        .then(if (c == selected) Modifier.border(2.dp, shelf.fg, CircleShape).padding(4.dp) else Modifier)
                        .clip(CircleShape).background(highlightColor(c)),
                )
            }
        }
    }
}

/**
 * The highlights and notes of a book: search, filters, grouped by chapter in
 * reading order. Used in the reader and on the book's page.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AnnotationList(
    list: List<Annotation>,
    onOpen: ((Annotation) -> Unit)?,
    onEdit: (Annotation) -> Unit,
    onDelete: (Annotation) -> Unit,
    onShare: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var query by remember { mutableStateOf("") }
    var onlyNotes by remember { mutableStateOf(false) }
    var color by remember { mutableStateOf<String?>(null) }
    Column(modifier) {
        if (list.isEmpty()) {
            Column(Modifier.fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.xxl), horizontalAlignment = Alignment.CenterHorizontally) {
                Icon(Icons.Outlined.EditNote, null, tint = shelf.muted, modifier = Modifier.size(32.dp))
                Spacer(Modifier.height(Space.m))
                Text(stringResource(R.string.no_highlights), color = shelf.fg, fontSize = TextSize.subtitle, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(Space.xs))
                Text(
                    stringResource(R.string.no_highlights_text), color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp,
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                )
            }
            return@Column
        }
        val notes = list.count { it.hasNote }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                pluralStringResource(R.plurals.highlights_count, list.size, list.size, notes),
                color = shelf.muted, fontSize = TextSize.bodySmall, modifier = Modifier.weight(1f),
            )
            IconButton(onClick = onShare) { Icon(Icons.Outlined.Share, stringResource(R.string.share_highlights), tint = shelf.fg) }
        }
        if (list.size > 3) {
            OutlinedTextField(
                query, { query = it }, singleLine = true,
                placeholder = { Text(stringResource(R.string.search_highlights), fontSize = TextSize.bodySmall) },
                leadingIcon = { Icon(Icons.Outlined.Search, null, tint = shelf.muted) },
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = shelf.fg, unfocusedBorderColor = shelf.line, focusedTextColor = shelf.fg, unfocusedTextColor = shelf.fg,
                    cursorColor = shelf.fg,
                ),
                modifier = Modifier.fillMaxWidth(),
            )
        }
        val used = Annotation.COLORS.keys.filter { c -> list.any { it.color == c } }
        if (notes > 0 || used.size > 1) {
            Spacer(Modifier.height(Space.xs))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
                Pill(stringResource(R.string.all_highlights), !onlyNotes && color == null) { onlyNotes = false; color = null }
                if (notes > 0) Pill(stringResource(R.string.with_note), onlyNotes) { onlyNotes = !onlyNotes }
                if (used.size > 1) used.forEach { c ->
                    Pill(stringResource(colorNames[c] ?: R.string.color_yellow), color == c) { color = if (color == c) null else c }
                }
            }
        }
        val q = query.trim().lowercase()
        val shown = list.filter { a ->
            (!onlyNotes || a.hasNote) && (color == null || a.color == color) &&
                (q.isEmpty() || "${a.text} ${a.note} ${a.chapter}".lowercase().contains(q))
        }
        if (shown.isEmpty()) {
            Text(
                stringResource(R.string.nothing_matches), color = shelf.muted, fontSize = TextSize.bodySmall,
                modifier = Modifier.padding(vertical = Space.xl).fillMaxWidth(),
                textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            )
        }
        val noChapter = stringResource(R.string.no_chapter)
        var chapter: String? = null
        shown.forEach { a ->
            if (a.chapter != chapter) {
                chapter = a.chapter
                Spacer(Modifier.height(Space.m))
                Text(
                    a.chapter.ifBlank { noChapter }, color = shelf.muted, fontSize = TextSize.caption, fontWeight = FontWeight.Bold,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(vertical = Space.xs),
                )
            }
            AnnotationCard(a, onOpen, onEdit, onDelete)
            Spacer(Modifier.height(Space.s))
        }
    }
}

@Composable
private fun AnnotationCard(a: Annotation, onOpen: ((Annotation) -> Unit)?, onEdit: (Annotation) -> Unit, onDelete: (Annotation) -> Unit) {
    val goTo = stringResource(R.string.go_to_highlight)
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).border(1.dp, shelf.line, RoundedCornerShape(8.dp))
            .then(if (onOpen != null) Modifier.clickable(onClickLabel = goTo) { onOpen(a) } else Modifier),
    ) {
        Box(Modifier.width(4.dp).heightIn(min = 72.dp).padding(vertical = Space.m).background(highlightColor(a.color)))
        Column(Modifier.weight(1f).padding(start = Space.m, top = Space.m, end = Space.xs, bottom = Space.xs)) {
            Text(
                a.text.ifBlank { "—" }, color = shelf.fg, fontSize = TextSize.body, lineHeight = 23.sp, fontFamily = FontFamily.Serif,
                maxLines = 8, overflow = TextOverflow.Ellipsis,
                textDecoration = if (a.style == "underscore") androidx.compose.ui.text.style.TextDecoration.Underline
                else if (a.style == "strikeout") androidx.compose.ui.text.style.TextDecoration.LineThrough else null,
            )
            if (a.hasNote) {
                Spacer(Modifier.height(Space.s))
                Text(
                    a.note.trim(), color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp,
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(6.dp)).background(shelf.soft).padding(Space.s),
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    listOfNotNull(a.page?.let { "p. $it" }, percent(a.percentage), a.device.takeIf { it.isNotBlank() }, relativeTime(a.updated * 1000))
                        .joinToString(" · "),
                    color = shelf.muted, fontSize = TextSize.caption, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
                IconButton(onClick = { onEdit(a) }) {
                    Icon(Icons.Outlined.Edit, stringResource(R.string.edit_note), tint = shelf.muted, modifier = Modifier.size(20.dp))
                }
                IconButton(onClick = { onDelete(a) }) {
                    Icon(Icons.Outlined.Delete, stringResource(R.string.delete_highlight), tint = shelf.muted, modifier = Modifier.size(20.dp))
                }
            }
        }
    }
}

/** Writes or changes the note and colour of a highlight. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AnnotationEditor(
    annotation: Annotation,
    isNew: Boolean,
    onSave: (Annotation) -> Unit,
    onDelete: () -> Unit,
    onDismiss: () -> Unit,
) {
    var note by remember(annotation.id) { mutableStateOf(annotation.note) }
    var color by remember(annotation.id) { mutableStateOf(annotation.color) }
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = shelf.pane, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.padding(horizontal = Space.xl).padding(bottom = Space.xl)) {
            Text(
                stringResource(if (isNew) R.string.new_note else R.string.edit_note),
                color = shelf.fg, fontSize = TextSize.title, fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.height(Space.m))
            Row {
                Box(Modifier.width(3.dp).heightIn(min = 20.dp).background(highlightColor(color)))
                Text(
                    annotation.text, color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp, fontFamily = FontFamily.Serif,
                    maxLines = 6, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = Space.m),
                )
            }
            Spacer(Modifier.height(Space.l))
            OutlinedTextField(
                note, { note = it }, minLines = 3, maxLines = 10,
                placeholder = { Text(stringResource(R.string.note_hint)) },
                colors = OutlinedTextFieldDefaults.colors(
                    focusedBorderColor = shelf.fg, unfocusedBorderColor = shelf.line, focusedTextColor = shelf.fg, unfocusedTextColor = shelf.fg,
                    cursorColor = shelf.fg,
                ),
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(Space.s))
            Row(verticalAlignment = Alignment.CenterVertically) {
                ColorPicker(color, { color = it }, Modifier.weight(1f))
                if (!isNew) {
                    IconButton(onClick = onDelete) { Icon(Icons.Outlined.Delete, stringResource(R.string.delete_highlight), tint = shelf.muted) }
                }
            }
            Spacer(Modifier.height(Space.m))
            Row(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
                SecondaryButton(stringResource(R.string.cancel), onDismiss, Modifier.weight(1f))
                PrimaryButton(stringResource(R.string.save), { onSave(annotation.copy(note = note.trim(), color = color)) }, Modifier.weight(1f))
            }
        }
    }
}

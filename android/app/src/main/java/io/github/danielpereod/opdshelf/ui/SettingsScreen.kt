package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Logout
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.BuildConfig
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app

@Composable
fun SettingsScreen(library: LibraryViewModel, onSaved: () -> Unit, onDisconnect: () -> Unit, modifier: Modifier) {
    val context = LocalContext.current
    val settings = context.app.settings
    val status by library.syncStatus.collectAsState()
    val books by library.books.collectAsState()
    var device by remember { mutableStateOf(settings.deviceName) }
    var editServer by remember { mutableStateOf(!settings.isConfigured) }
    var confirmDisconnect by remember { mutableStateOf(false) }
    var confirmRemoveAll by remember { mutableStateOf(false) }
    val clipboard = androidx.compose.ui.platform.LocalClipboardManager.current

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(stringResource(R.string.tab_settings))
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding(),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
          Column(Modifier.widthIn(max = ReadableWidth).fillMaxWidth().padding(horizontal = edgePadding, vertical = Space.s)) {
            SectionTitle(stringResource(R.string.app_theme), Modifier.padding(top = Space.l))
            Row(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
                listOf(R.string.theme_system, R.string.theme_light, R.string.theme_dark).forEachIndexed { i, label ->
                    Pill(stringResource(label), AppearanceState.mode == i) {
                        AppearanceState.mode = i
                        settings.appearance = i
                    }
                }
            }

            SectionTitle(stringResource(R.string.settings_server), Modifier.padding(top = Space.xxl))
            if (editServer) {
                ConnectForm(
                    onConnected = { editServer = false; onSaved() },
                    buttonText = stringResource(R.string.save_and_check),
                )
            } else {
                InfoRow(stringResource(R.string.server), settings.serverUrl)
                InfoRow(stringResource(R.string.username), settings.username.ifEmpty { "—" })
                Spacer(Modifier.height(Space.l))
                SecondaryButton(stringResource(R.string.change_server), { editServer = true }, Modifier.fillMaxWidth())
            }

            SectionTitle(stringResource(R.string.settings_sync), Modifier.padding(top = Space.xxl))
            SyncLine(status, onSync = { library.syncNow() })
            Spacer(Modifier.height(Space.xl))
            LabeledField(
                stringResource(R.string.device_name), device,
                { device = it; settings.deviceName = it },
                help = stringResource(R.string.device_name_help),
            )
            Spacer(Modifier.height(Space.m))
            Text(stringResource(R.string.sync_short), color = shelf.muted, fontSize = TextSize.label, lineHeight = 20.sp)
            if (settings.isConfigured) {
                val url = settings.serverUrl + "/kosync"
                val copied = stringResource(R.string.copied)
                Spacer(Modifier.height(Space.l))
                Row(
                    Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(shelf.soft)
                        .padding(start = Space.l, end = Space.xs, top = Space.s, bottom = Space.s),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(R.string.koreader_url), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
                        Text(url, color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp, fontFamily = Mono)
                    }
                    IconButton(onClick = {
                        clipboard.setText(androidx.compose.ui.text.AnnotatedString(url))
                        android.widget.Toast.makeText(context, copied, android.widget.Toast.LENGTH_SHORT).show()
                    }) {
                        Icon(Icons.Outlined.ContentCopy, stringResource(R.string.copy), tint = shelf.fg)
                    }
                }
                Spacer(Modifier.height(Space.s))
                Text(stringResource(R.string.koreader_short), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
            }

            SectionTitle(stringResource(R.string.settings_storage), Modifier.padding(top = Space.xxl))
            InfoRow(
                pluralStringResource(R.plurals.books_on_device, books.size, books.size),
                formatSize(remember(books) { context.app.store.usedBytes() }) ?: "0 B",
            )
            if (books.isNotEmpty()) {
                Spacer(Modifier.height(Space.l))
                SecondaryButton(
                    stringResource(R.string.remove_all), { confirmRemoveAll = true }, Modifier.fillMaxWidth(),
                    destructive = true, icon = Icons.Outlined.Delete,
                )
            }

            SectionTitle(stringResource(R.string.settings_about), Modifier.padding(top = Space.xxl))
            InfoRow(stringResource(R.string.version), BuildConfig.VERSION_NAME)
            val uriHandler = androidx.compose.ui.platform.LocalUriHandler.current
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp)
                    .clickable(role = androidx.compose.ui.semantics.Role.Button) { uriHandler.openUri("https://github.com/DanielPereod/opdshelf") }
                    .padding(vertical = Space.m),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(stringResource(R.string.project_link), color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp, modifier = Modifier.weight(1f))
                Icon(Icons.AutoMirrored.Outlined.OpenInNew, null, tint = shelf.muted, modifier = Modifier.size(20.dp))
            }
            HorizontalDivider(color = shelf.line)

            if (settings.isConfigured) {
                Spacer(Modifier.height(Space.xxl + Space.l))
                SecondaryButton(
                    stringResource(R.string.disconnect), { confirmDisconnect = true }, Modifier.fillMaxWidth(),
                    destructive = true, icon = Icons.AutoMirrored.Outlined.Logout,
                )
            }
            Spacer(Modifier.height(Space.xl))
          }
        }
    }

    if (confirmDisconnect) {
        ConfirmDialog(
            title = stringResource(R.string.disconnect_title),
            text = stringResource(R.string.disconnect_text),
            confirm = stringResource(R.string.disconnect_short),
            onConfirm = {
                confirmDisconnect = false
                settings.forgetServer()
                onDisconnect()
            },
            onDismiss = { confirmDisconnect = false },
        )
    }
    if (confirmRemoveAll) {
        ConfirmDialog(
            title = stringResource(R.string.remove_all_title),
            text = stringResource(R.string.remove_all_text),
            confirm = stringResource(R.string.remove),
            onConfirm = {
                confirmRemoveAll = false
                library.deleteAll()
            },
            onDismiss = { confirmRemoveAll = false },
        )
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    Column {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(vertical = Space.m).semantics(mergeDescendants = true) {},
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(label, color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp)
            Spacer(Modifier.width(Space.l))
            Text(
                value, color = shelf.fg, fontSize = TextSize.bodySmall, lineHeight = 20.sp, fontWeight = FontWeight.Medium,
                maxLines = 1, overflow = TextOverflow.MiddleEllipsis, textAlign = androidx.compose.ui.text.style.TextAlign.End,
                modifier = Modifier.weight(1f),
            )
        }
        HorizontalDivider(color = shelf.line)
    }
}

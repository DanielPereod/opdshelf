package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.background
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

    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(stringResource(R.string.tab_settings))
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding(),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
          Column(Modifier.widthIn(max = ReadableWidth).fillMaxWidth().padding(horizontal = edgePadding, vertical = Space.s)) {
            SectionTitle(stringResource(R.string.settings_server), Modifier.padding(top = Space.l))
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
            Spacer(Modifier.height(Space.l))
            Text(stringResource(R.string.sync_explained), color = shelf.muted, fontSize = TextSize.label, lineHeight = 20.sp)
            if (settings.isConfigured) {
                Spacer(Modifier.height(Space.s))
                Text(
                    stringResource(R.string.koreader_hint, settings.serverUrl + "/kosync"),
                    color = shelf.muted, fontSize = TextSize.label, lineHeight = 20.sp,
                )
            }

            SectionTitle(stringResource(R.string.settings_storage), Modifier.padding(top = Space.xxl))
            InfoRow(
                pluralStringResource(R.plurals.books_on_device, books.size, books.size),
                formatSize(remember(books) { context.app.store.usedBytes() }) ?: "0 B",
            )

            SectionTitle(stringResource(R.string.settings_about), Modifier.padding(top = Space.xxl))
            InfoRow(stringResource(R.string.version), BuildConfig.VERSION_NAME)

            if (settings.isConfigured) {
                Spacer(Modifier.height(Space.xxl))
                SecondaryButton(stringResource(R.string.disconnect), { confirmDisconnect = true }, Modifier.fillMaxWidth())
            }
            Spacer(Modifier.height(Space.xl))
          }
        }
    }

    if (confirmDisconnect) {
        AlertDialog(
            onDismissRequest = { confirmDisconnect = false },
            title = { Text(stringResource(R.string.disconnect_title)) },
            text = { Text(stringResource(R.string.disconnect_text)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmDisconnect = false
                    settings.forgetServer()
                    onDisconnect()
                }) { Text(stringResource(R.string.disconnect), color = shelf.fg, fontWeight = FontWeight.SemiBold) }
            },
            dismissButton = { TextButton(onClick = { confirmDisconnect = false }) { Text(stringResource(R.string.cancel), color = shelf.muted) } },
            containerColor = shelf.pane,
            shape = RoundedCornerShape(8.dp),
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

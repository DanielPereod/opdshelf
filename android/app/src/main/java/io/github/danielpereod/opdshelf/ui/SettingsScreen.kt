package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.background
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
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(horizontal = 20.dp, vertical = 8.dp),
        ) {
            SectionTitle(stringResource(R.string.settings_server), Modifier.padding(top = 16.dp))
            if (editServer) {
                ConnectForm(
                    onConnected = { editServer = false; onSaved() },
                    buttonText = stringResource(R.string.save_and_check),
                )
            } else {
                InfoRow(stringResource(R.string.server), settings.serverUrl)
                InfoRow(stringResource(R.string.username), settings.username.ifEmpty { "—" })
                Spacer(Modifier.height(12.dp))
                SecondaryButton(stringResource(R.string.change_server), { editServer = true }, Modifier.fillMaxWidth())
            }

            SectionTitle(stringResource(R.string.settings_sync), Modifier.padding(top = 28.dp))
            SyncLine(status, onSync = { library.syncNow() })
            Spacer(Modifier.height(16.dp))
            LabeledField(
                stringResource(R.string.device_name), device,
                { device = it; settings.deviceName = it },
                help = stringResource(R.string.device_name_help),
            )
            Spacer(Modifier.height(12.dp))
            Text(stringResource(R.string.sync_explained), color = shelf.muted, fontSize = 13.sp, lineHeight = 19.sp)
            if (settings.isConfigured) {
                Spacer(Modifier.height(8.dp))
                Text(
                    stringResource(R.string.koreader_hint, settings.serverUrl + "/kosync"),
                    color = shelf.muted, fontSize = 13.sp, lineHeight = 19.sp,
                )
            }

            SectionTitle(stringResource(R.string.settings_storage), Modifier.padding(top = 28.dp))
            InfoRow(
                pluralStringResource(R.plurals.books_on_device, books.size, books.size),
                formatSize(remember(books) { context.app.store.usedBytes() }) ?: "0 B",
            )

            SectionTitle(stringResource(R.string.settings_about), Modifier.padding(top = 28.dp))
            InfoRow(stringResource(R.string.version), BuildConfig.VERSION_NAME)

            if (settings.isConfigured) {
                Spacer(Modifier.height(28.dp))
                HorizontalDivider(color = shelf.line)
                TextButton(onClick = { confirmDisconnect = true }, modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
                    Text(stringResource(R.string.disconnect), color = shelf.fg, fontWeight = FontWeight.SemiBold)
                }
            }
            Spacer(Modifier.height(24.dp))
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
        Row(Modifier.fillMaxWidth().padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween) {
            Text(label, color = shelf.muted, fontSize = 14.sp)
            Spacer(Modifier.padding(horizontal = 8.dp))
            Text(value, color = shelf.fg, fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1)
        }
        HorizontalDivider(color = shelf.line)
    }
}

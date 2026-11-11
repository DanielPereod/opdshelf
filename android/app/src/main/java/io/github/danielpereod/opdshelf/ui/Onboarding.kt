package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.MenuBook
import androidx.compose.material.icons.outlined.CloudSync
import androidx.compose.material.icons.outlined.FormatQuote
import androidx.compose.material.icons.outlined.Visibility
import androidx.compose.material.icons.outlined.VisibilityOff
import androidx.compose.material.icons.outlined.WifiOff
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import kotlinx.coroutines.launch

/** First screen of a new install: what the app does, before asking for anything. */
@Composable
fun WelcomeScreen(onStart: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier.fillMaxSize().background(shelf.bg).verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 40.dp),
    ) {
        Image(painterResource(R.drawable.logo), null, Modifier.size(56.dp))
        Spacer(Modifier.height(24.dp))
        Text(stringResource(R.string.welcome_title), color = shelf.fg, fontSize = 28.sp, fontWeight = FontWeight.Bold, lineHeight = 34.sp)
        Spacer(Modifier.height(8.dp))
        Text(stringResource(R.string.welcome_text), color = shelf.muted, fontSize = 15.sp, lineHeight = 22.sp)
        Spacer(Modifier.height(32.dp))
        Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Feature(Icons.AutoMirrored.Outlined.MenuBook, R.string.feature_library_title, R.string.feature_library_text)
            Feature(Icons.Outlined.WifiOff, R.string.feature_offline_title, R.string.feature_offline_text)
            Feature(Icons.Outlined.CloudSync, R.string.feature_sync_title, R.string.feature_sync_text)
            Feature(Icons.Outlined.FormatQuote, R.string.feature_mark_title, R.string.feature_mark_text)
        }
        Spacer(Modifier.height(40.dp))
        PrimaryButton(stringResource(R.string.connect_server), onStart, Modifier.fillMaxWidth(), height = 48.dp)
        Spacer(Modifier.height(12.dp))
        Text(stringResource(R.string.welcome_need_server), color = shelf.muted, fontSize = 12.sp, lineHeight = 17.sp)
    }
}

@Composable
private fun Feature(icon: ImageVector, title: Int, text: Int) {
    Row {
        Box(Modifier.size(40.dp).clip(RoundedCornerShape(8.dp)).background(shelf.soft), contentAlignment = Alignment.Center) {
            Icon(icon, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(stringResource(title), color = shelf.fg, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
            Text(stringResource(text), color = shelf.muted, fontSize = 13.sp, lineHeight = 18.sp)
        }
    }
}

/**
 * Server address and login, checked against the server before saving.
 * Used on first run and in Settings.
 */
@Composable
fun ConnectForm(onConnected: () -> Unit, buttonText: String, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val settings = context.app.settings
    var server by remember { mutableStateOf(settings.serverUrl) }
    var username by remember { mutableStateOf(settings.username) }
    var password by remember { mutableStateOf(settings.password) }
    var showPassword by remember { mutableStateOf(false) }
    var checking by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    Column(modifier, verticalArrangement = Arrangement.spacedBy(16.dp)) {
        LabeledField(
            stringResource(R.string.server), server, { server = it; error = null },
            placeholder = "192.168.1.10:3000", keyboardType = KeyboardType.Uri,
            help = stringResource(R.string.server_help),
        )
        LabeledField(stringResource(R.string.username), username, { username = it; error = null })
        LabeledField(
            stringResource(R.string.password), password, { password = it; error = null },
            keyboardType = KeyboardType.Password,
            visualTransformation = if (showPassword) androidx.compose.ui.text.input.VisualTransformation.None else PasswordVisualTransformation(),
            trailing = {
                PasswordToggle(showPassword) { showPassword = !showPassword }
            },
        )
        error?.let {
            Text(
                it, color = shelf.fg, fontSize = 13.sp, lineHeight = 18.sp,
                modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(4.dp)).background(shelf.soft)
                    .border(1.dp, shelf.lineStrong, RoundedCornerShape(4.dp)).padding(12.dp),
            )
        }
        PrimaryButton(
            text = if (checking) stringResource(R.string.checking) else buttonText,
            enabled = !checking && server.isNotBlank(),
            height = 48.dp,
            modifier = Modifier.fillMaxWidth(),
            onClick = {
                settings.serverUrl = server
                settings.username = username
                settings.password = password
                server = settings.serverUrl
                checking = true
                error = null
                scope.launch {
                    try {
                        context.app.api.check()
                        context.app.sync.schedule()
                        onConnected()
                    } catch (e: Exception) {
                        error = context.getString(R.string.cannot_connect, context.describe(e))
                    } finally {
                        checking = false
                    }
                }
            },
        )
    }
}

@Composable
private fun PasswordToggle(visible: Boolean, onToggle: () -> Unit) {
    androidx.compose.material3.IconButton(onClick = onToggle) {
        Icon(
            if (visible) Icons.Outlined.VisibilityOff else Icons.Outlined.Visibility,
            stringResource(if (visible) R.string.hide_password else R.string.show_password),
            tint = shelf.muted,
        )
    }
}

/** The connection step of the first run. */
@Composable
fun ConnectScreen(onConnected: () -> Unit, onBack: () -> Unit, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(stringResource(R.string.connect_title), onBack = onBack)
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(horizontal = 20.dp, vertical = 24.dp),
        ) {
            Text(stringResource(R.string.connect_text), color = shelf.muted, fontSize = 14.sp, lineHeight = 20.sp)
            Spacer(Modifier.height(24.dp))
            ConnectForm(onConnected, stringResource(R.string.connect))
        }
    }
}

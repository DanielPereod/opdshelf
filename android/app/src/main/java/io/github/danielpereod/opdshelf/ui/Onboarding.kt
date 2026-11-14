package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
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
import androidx.compose.material.icons.outlined.QrCodeScanner
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
        modifier.fillMaxSize().background(shelf.bg).verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
      Column(Modifier.widthIn(max = 560.dp).fillMaxWidth().padding(horizontal = Space.xl, vertical = Space.xxl + Space.s)) {
        Logo(56.dp)
        Spacer(Modifier.height(Space.xl))
        Text(
            stringResource(R.string.welcome_title), color = shelf.fg, fontSize = TextSize.display, fontWeight = FontWeight.Bold, lineHeight = 36.sp,
            modifier = Modifier.semantics { heading() },
        )
        Spacer(Modifier.height(Space.s))
        Text(stringResource(R.string.welcome_text), color = shelf.muted, fontSize = TextSize.subtitle, lineHeight = 24.sp)
        Spacer(Modifier.height(Space.xxl))
        Column(verticalArrangement = Arrangement.spacedBy(Space.xl)) {
            Feature(Icons.AutoMirrored.Outlined.MenuBook, R.string.feature_library_title, R.string.feature_library_text)
            Feature(Icons.Outlined.WifiOff, R.string.feature_offline_title, R.string.feature_offline_text)
            Feature(Icons.Outlined.CloudSync, R.string.feature_sync_title, R.string.feature_sync_text)
            Feature(Icons.Outlined.FormatQuote, R.string.feature_mark_title, R.string.feature_mark_text)
        }
        Spacer(Modifier.height(Space.xxl + Space.s))
        PrimaryButton(stringResource(R.string.connect_server), onStart, Modifier.fillMaxWidth(), height = 48.dp)
        Spacer(Modifier.height(Space.m))
        Text(
            stringResource(R.string.welcome_need_server), color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp,
            textAlign = androidx.compose.ui.text.style.TextAlign.Center, modifier = Modifier.fillMaxWidth(),
        )
      }
    }
}

@Composable
private fun Feature(icon: ImageVector, title: Int, text: Int) {
    Row(Modifier.semantics(mergeDescendants = true) {}) {
        Box(Modifier.size(40.dp).clip(RoundedCornerShape(8.dp)).background(shelf.soft), contentAlignment = Alignment.Center) {
            Icon(icon, null, tint = shelf.fg, modifier = Modifier.size(20.dp))
        }
        Spacer(Modifier.width(Space.l))
        Column(Modifier.weight(1f)) {
            Text(stringResource(title), color = shelf.fg, fontSize = TextSize.body, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold)
            Text(stringResource(text), color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp)
        }
    }
}

/**
 * Server address and login, checked against the server before saving.
 * Used on first run and in Settings.
 */
@Composable
fun ConnectForm(
    onConnected: () -> Unit,
    buttonText: String,
    modifier: Modifier = Modifier,
    link: io.github.danielpereod.opdshelf.data.ConnectLink? = null,
) {
    val context = LocalContext.current
    val settings = context.app.settings
    var server by remember { mutableStateOf(link?.server ?: settings.serverUrl) }
    var username by remember { mutableStateOf(link?.username ?: settings.username) }
    var password by remember { mutableStateOf(link?.password ?: settings.password) }
    var showPassword by remember { mutableStateOf(false) }
    var checking by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    // Which field the error is about, to mark it
    var badLogin by remember { mutableStateOf(false) }
    var badServer by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val focus = androidx.compose.ui.platform.LocalFocusManager.current
    val submit = {
        if (!checking && server.isNotBlank()) {
            focus.clearFocus()
            settings.serverUrl = server
            settings.username = username
            settings.password = password
            server = settings.serverUrl
            checking = true
            error = null
            badLogin = false
            badServer = false
            scope.launch {
                try {
                    context.app.api.check()
                    context.app.sync.schedule()
                    onConnected()
                } catch (e: Exception) {
                    error = context.getString(R.string.cannot_connect, context.describe(e))
                    badLogin = e is io.github.danielpereod.opdshelf.data.HttpError && e.code == 401
                    badServer = !badLogin
                } finally {
                    checking = false
                }
            }
        }
    }

    // The panel's QR code fills in the form and connects
    val scan = {
        error = null
        val options = com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions.Builder()
            .setBarcodeFormats(com.google.mlkit.vision.barcode.common.Barcode.FORMAT_QR_CODE)
            .build()
        com.google.mlkit.vision.codescanner.GmsBarcodeScanning.getClient(context, options).startScan()
            .addOnSuccessListener { code ->
                val scanned = io.github.danielpereod.opdshelf.data.ConnectLink.parse(code.rawValue)
                if (scanned == null) {
                    error = context.getString(R.string.scan_not_opdshelf)
                } else {
                    server = scanned.server
                    username = scanned.username
                    password = scanned.password
                    submit()
                }
            }
            .addOnFailureListener { error = context.getString(R.string.scan_unavailable) }
        Unit
    }

    Column(modifier, verticalArrangement = Arrangement.spacedBy(Space.l)) {
        SecondaryButton(
            stringResource(R.string.scan_qr), scan, Modifier.fillMaxWidth(),
            icon = Icons.Outlined.QrCodeScanner,
        )
        LabeledField(
            stringResource(R.string.server), server, { server = it; error = null; badServer = false },
            placeholder = "192.168.1.10:3000", keyboardType = KeyboardType.Uri,
            help = stringResource(R.string.server_help),
            imeAction = androidx.compose.ui.text.input.ImeAction.Next, isError = badServer, autoCorrect = false,
        )
        LabeledField(
            stringResource(R.string.username), username, { username = it; error = null; badLogin = false },
            imeAction = androidx.compose.ui.text.input.ImeAction.Next, isError = badLogin,
            autofill = androidx.compose.ui.autofill.ContentType.Username, autoCorrect = false,
        )
        LabeledField(
            stringResource(R.string.password), password, { password = it; error = null; badLogin = false },
            keyboardType = KeyboardType.Password,
            imeAction = androidx.compose.ui.text.input.ImeAction.Done, onImeAction = submit, isError = badLogin,
            autofill = androidx.compose.ui.autofill.ContentType.Password, autoCorrect = false,
            visualTransformation = if (showPassword) androidx.compose.ui.text.input.VisualTransformation.None else PasswordVisualTransformation(),
            trailing = {
                PasswordToggle(showPassword) { showPassword = !showPassword }
            },
        )
        error?.let {
            Row(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(8.dp)).background(shelf.soft)
                    .border(1.dp, shelf.danger, RoundedCornerShape(8.dp)).padding(Space.m)
                    .semantics { liveRegion = androidx.compose.ui.semantics.LiveRegionMode.Polite },
            ) {
                Icon(Icons.Outlined.ErrorOutline, null, tint = shelf.danger, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(Space.m))
                Text(it, color = shelf.fg, fontSize = TextSize.label, lineHeight = 20.sp, modifier = Modifier.weight(1f))
            }
        }
        PrimaryButton(
            text = if (checking) stringResource(R.string.checking) else buttonText,
            enabled = !checking && server.isNotBlank(),
            height = 48.dp,
            modifier = Modifier.fillMaxWidth(),
            onClick = submit,
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
fun ConnectScreen(
    onConnected: () -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    link: io.github.danielpereod.opdshelf.data.ConnectLink? = null,
) {
    Column(modifier.fillMaxSize().background(shelf.bg)) {
        Header(stringResource(R.string.connect_title), onBack = onBack)
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding(),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
          Column(Modifier.widthIn(max = 560.dp).fillMaxWidth().padding(horizontal = edgePadding, vertical = Space.xl)) {
            Text(stringResource(R.string.connect_text), color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp)
            Spacer(Modifier.height(Space.xl))
            ConnectForm(onConnected, stringResource(R.string.connect), link = link)
          }
        }
    }
}

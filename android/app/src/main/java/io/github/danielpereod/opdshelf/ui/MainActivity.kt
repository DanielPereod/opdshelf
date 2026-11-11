package io.github.danielpereod.opdshelf.ui

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.automirrored.outlined.LibraryBooks
import androidx.compose.material.icons.outlined.DownloadDone
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.material.icons.Icons
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.LocalBook
import io.github.danielpereod.opdshelf.reader.ReaderActivity

class MainActivity : ComponentActivity() {
    private val library: LibraryViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { AppTheme { Main(library, ::openBook) } }
    }

    private fun openBook(book: LocalBook) {
        startActivity(Intent(this, ReaderActivity::class.java).putExtra(ReaderActivity.EXTRA_BOOK_ID, book.id))
    }
}

enum class Tab { Library, Device, Settings }

@Composable
fun Main(library: LibraryViewModel, openBook: (LocalBook) -> Unit) {
    val context = LocalContext.current
    var configured by remember { mutableStateOf(context.app.settings.isConfigured) }
    var tab by rememberSaveable { mutableStateOf(if (configured) Tab.Device else Tab.Settings) }
    val snackbar = remember { SnackbarHostState() }
    val message by library.messages.collectAsState()
    LaunchedEffect(message) {
        message?.let {
            snackbar.showSnackbar(it)
            library.messageShown()
        }
    }
    LaunchedEffect(configured) { if (configured) library.openRoot() }

    BackHandler(enabled = tab == Tab.Library && library.stack.collectAsState().value.size > 1) { library.back() }

    Scaffold(
        containerColor = shelf.bg,
        snackbarHost = { SnackbarHost(snackbar) },
        bottomBar = {
            Column {
                HorizontalDivider(color = shelf.line)
                NavigationBar(containerColor = shelf.pane, tonalElevation = 0.dp) {
                    val colors = NavigationBarItemDefaults.colors(
                        selectedIconColor = shelf.fg, selectedTextColor = shelf.fg, indicatorColor = shelf.soft,
                        unselectedIconColor = shelf.muted, unselectedTextColor = shelf.muted,
                    )
                    NavigationBarItem(
                        selected = tab == Tab.Library, onClick = { tab = Tab.Library }, enabled = configured, colors = colors,
                        icon = { Icon(Icons.AutoMirrored.Outlined.LibraryBooks, null) },
                        label = { Text(stringResource(R.string.tab_library), fontWeight = FontWeight.Medium) },
                    )
                    NavigationBarItem(
                        selected = tab == Tab.Device, onClick = { tab = Tab.Device }, enabled = configured, colors = colors,
                        icon = { Icon(Icons.Outlined.DownloadDone, null) },
                        label = { Text(stringResource(R.string.tab_device), fontWeight = FontWeight.Medium) },
                    )
                    NavigationBarItem(
                        selected = tab == Tab.Settings, onClick = { tab = Tab.Settings }, colors = colors,
                        icon = { Icon(Icons.Outlined.Settings, null) },
                        label = { Text(stringResource(R.string.tab_settings), fontWeight = FontWeight.Medium) },
                    )
                }
            }
        },
    ) { padding ->
        val modifier = Modifier.padding(padding)
        when (tab) {
            Tab.Library -> CatalogScreen(library, openBook, modifier)
            Tab.Device -> DeviceScreen(library, openBook, modifier)
            Tab.Settings -> SettingsScreen(modifier) {
                configured = true
                library.openRoot()
                tab = Tab.Library
            }
        }
    }
}

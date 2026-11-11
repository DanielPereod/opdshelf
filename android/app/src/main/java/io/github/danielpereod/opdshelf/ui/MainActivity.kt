package io.github.danielpereod.opdshelf.ui

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.LibraryBooks
import androidx.compose.material.icons.outlined.Explore
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Snackbar
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
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

enum class Tab { Home, Explore, Settings }

private enum class Setup { Welcome, Connect }

@Composable
fun Main(library: LibraryViewModel, openBook: (LocalBook) -> Unit) {
    val context = LocalContext.current
    var configured by remember { mutableStateOf(context.app.settings.isConfigured) }
    var setup by rememberSaveable { mutableStateOf(Setup.Welcome) }

    if (!configured) {
        AnimatedContent(setup, transitionSpec = { fadeIn() togetherWith fadeOut() }, label = "setup") { step ->
            when (step) {
                Setup.Welcome -> Scaffold(containerColor = shelf.bg) { WelcomeScreen({ setup = Setup.Connect }, Modifier.padding(it)) }
                Setup.Connect -> {
                    BackHandler { setup = Setup.Welcome }
                    Scaffold(containerColor = shelf.bg) {
                        ConnectScreen(
                            onConnected = { configured = true },
                            onBack = { setup = Setup.Welcome },
                            modifier = Modifier.padding(it),
                        )
                    }
                }
            }
        }
        return
    }

    var tab by rememberSaveable { mutableStateOf(if (context.app.store.books.value.isEmpty()) Tab.Explore else Tab.Home) }
    val snackbar = remember { SnackbarHostState() }
    val message by library.messages.collectAsState()
    val detail by library.detail.collectAsState()
    val stack by library.stack.collectAsState()
    LaunchedEffect(message) {
        message?.let {
            snackbar.showSnackbar(it)
            library.messageShown()
        }
    }
    LaunchedEffect(Unit) { library.openRoot() }

    BackHandler(enabled = detail != null) { library.closeDetail() }
    BackHandler(enabled = detail == null && tab == Tab.Explore && stack.size > 1) { library.back() }
    BackHandler(enabled = detail == null && tab != Tab.Home && !(tab == Tab.Explore && stack.size > 1)) { tab = Tab.Home }

    Scaffold(
        containerColor = shelf.bg,
        snackbarHost = {
            SnackbarHost(snackbar) { Snackbar(it, containerColor = shelf.fg, contentColor = shelf.inverse, shape = androidx.compose.foundation.shape.RoundedCornerShape(6.dp)) }
        },
        bottomBar = {
            if (detail == null) {
                Column {
                    HorizontalDivider(color = shelf.line)
                    NavigationBar(containerColor = shelf.pane, tonalElevation = 0.dp) {
                        val colors = NavigationBarItemDefaults.colors(
                            selectedIconColor = shelf.fg, selectedTextColor = shelf.fg, indicatorColor = shelf.soft,
                            unselectedIconColor = shelf.muted, unselectedTextColor = shelf.muted,
                        )
                        NavigationBarItem(
                            selected = tab == Tab.Home, onClick = { tab = Tab.Home }, colors = colors,
                            icon = { Icon(Icons.AutoMirrored.Outlined.LibraryBooks, null) },
                            label = { Text(stringResource(R.string.tab_home), fontWeight = FontWeight.Medium) },
                        )
                        NavigationBarItem(
                            selected = tab == Tab.Explore, onClick = { tab = Tab.Explore }, colors = colors,
                            icon = { Icon(Icons.Outlined.Explore, null) },
                            label = { Text(stringResource(R.string.tab_explore), fontWeight = FontWeight.Medium) },
                        )
                        NavigationBarItem(
                            selected = tab == Tab.Settings, onClick = { tab = Tab.Settings }, colors = colors,
                            icon = { Icon(Icons.Outlined.Settings, null) },
                            label = { Text(stringResource(R.string.tab_settings), fontWeight = FontWeight.Medium) },
                        )
                    }
                }
            }
        },
    ) { padding ->
        val modifier = Modifier.padding(padding)
        val current = detail
        if (current != null) {
            DetailScreen(library, current, openBook, modifier)
            return@Scaffold
        }
        when (tab) {
            Tab.Home -> HomeScreen(
                library, openBook,
                showDetail = { library.showDetail(null, it.id) },
                goExplore = { tab = Tab.Explore },
                modifier = modifier,
            )
            Tab.Explore -> ExploreScreen(library, openBook, modifier)
            Tab.Settings -> SettingsScreen(
                library,
                onSaved = { library.openRoot() },
                onDisconnect = {
                    configured = false
                    setup = Setup.Welcome
                },
                modifier = modifier,
            )
        }
    }
}

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
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.NavigationRailItemDefaults
import androidx.compose.ui.res.painterResource
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

    val wide = isWide
    val tabs = listOf(
        Triple(Tab.Home, Icons.AutoMirrored.Outlined.LibraryBooks, R.string.tab_home),
        Triple(Tab.Explore, Icons.Outlined.Explore, R.string.tab_explore),
        Triple(Tab.Settings, Icons.Outlined.Settings, R.string.tab_settings),
    )
    val select: (Tab) -> Unit = {
        if (detail != null) library.closeDetail()
        tab = it
    }
    Scaffold(
        containerColor = shelf.bg,
        snackbarHost = {
            SnackbarHost(snackbar) { Snackbar(it, containerColor = shelf.fg, contentColor = shelf.inverse, shape = androidx.compose.foundation.shape.RoundedCornerShape(6.dp)) }
        },
        bottomBar = {
            // Phones: a bar at the bottom. Wide screens get a rail at the side instead.
            if (detail == null && !wide) {
                Column {
                    HorizontalDivider(color = shelf.line)
                    NavigationBar(containerColor = shelf.pane, tonalElevation = 0.dp) {
                        val colors = NavigationBarItemDefaults.colors(
                            selectedIconColor = shelf.fg, selectedTextColor = shelf.fg, indicatorColor = shelf.soft,
                            unselectedIconColor = shelf.muted, unselectedTextColor = shelf.muted,
                        )
                        tabs.forEach { (t, icon, label) ->
                            NavigationBarItem(
                                selected = tab == t, onClick = { select(t) }, colors = colors,
                                icon = { Icon(icon, null) },
                                label = { Text(stringResource(label), fontWeight = FontWeight.Medium, maxLines = 1) },
                            )
                        }
                    }
                }
            }
        },
    ) { padding ->
      Row(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)) {
        if (wide) {
            NavigationRail(
                containerColor = shelf.pane,
                header = { Image(painterResource(R.drawable.logo), null, Modifier.padding(vertical = 12.dp).size(32.dp)) },
            ) {
                val colors = NavigationRailItemDefaults.colors(
                    selectedIconColor = shelf.fg, selectedTextColor = shelf.fg, indicatorColor = shelf.soft,
                    unselectedIconColor = shelf.muted, unselectedTextColor = shelf.muted,
                )
                Spacer(Modifier.height(Space.s))
                tabs.forEach { (t, icon, label) ->
                    NavigationRailItem(
                        selected = tab == t && detail == null, onClick = { select(t) }, colors = colors,
                        icon = { Icon(icon, null) },
                        label = { Text(stringResource(label), fontWeight = FontWeight.Medium, maxLines = 1) },
                        modifier = Modifier.padding(vertical = Space.xs),
                    )
                }
            }
            androidx.compose.foundation.layout.Box(Modifier.fillMaxHeight().width(1.dp).background(shelf.line))
        }
        val modifier = Modifier.weight(1f)
        val current = detail
        if (current != null) {
            DetailScreen(library, current, openBook, modifier)
            return@Row
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
}

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
import androidx.compose.animation.core.tween
import androidx.compose.animation.scaleIn
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.ui.graphics.graphicsLayer


class MainActivity : ComponentActivity() {
    private val library: LibraryViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val link = io.github.danielpereod.opdshelf.data.ConnectLink.parse(intent?.dataString)
        setContent { AppTheme { Main(library, link, ::openBook) } }
        if (savedInstanceState == null) receiveShared(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        receiveShared(intent)
    }

    /** Books shared from another app (Files, a browser's downloads, mail) go to the upload sheet. */
    private fun receiveShared(intent: Intent?) {
        val uris = when (intent?.action) {
            Intent.ACTION_SEND -> listOfNotNull(
                androidx.core.content.IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, android.net.Uri::class.java),
            )
            Intent.ACTION_SEND_MULTIPLE -> androidx.core.content.IntentCompat
                .getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, android.net.Uri::class.java).orEmpty()
            else -> emptyList()
        }
        library.prepareUpload(uris)
    }

    private fun openBook(book: LocalBook) {
        startActivity(Intent(this, ReaderActivity::class.java).putExtra(ReaderActivity.EXTRA_BOOK_ID, book.id))
    }
}

enum class Tab { Home, Explore, Settings }

private enum class Setup { Welcome, Connect }

@Composable
fun Main(library: LibraryViewModel, link: io.github.danielpereod.opdshelf.data.ConnectLink? = null, openBook: (LocalBook) -> Unit) {
    val context = LocalContext.current
    // Status and navigation bar icons follow the app's appearance, which can differ from the system's
    val view = androidx.compose.ui.platform.LocalView.current
    val dark = shelf.isDark
    if (!view.isInEditMode) androidx.compose.runtime.SideEffect {
        (view.context as? android.app.Activity)?.window?.let { window ->
            androidx.core.view.WindowCompat.getInsetsController(window, view).apply {
                isAppearanceLightStatusBars = !dark
                isAppearanceLightNavigationBars = !dark
            }
        }
    }
    var configured by remember { mutableStateOf(context.app.settings.isConfigured) }
    // Opened from the panel's QR code: straight to the filled-in connect form
    var setup by rememberSaveable { mutableStateOf(if (link != null) Setup.Connect else Setup.Welcome) }

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
                            link = link,
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
    val actionLabel = message?.action?.let { stringResource(it) }
    LaunchedEffect(message) {
        message?.let {
            val result = snackbar.showSnackbar(it.text, actionLabel, withDismissAction = false, duration = androidx.compose.material3.SnackbarDuration.Long.takeIf { _ -> actionLabel != null } ?: androidx.compose.material3.SnackbarDuration.Short)
            library.messageShown()
            if (result == androidx.compose.material3.SnackbarResult.ActionPerformed) {
                it.book?.let(openBook)
                it.retry?.let { entry -> library.download(entry) }
            }
        }
    }
    LaunchedEffect(Unit) { library.openRoot() }
    val upload by library.upload.collectAsState()
    upload?.let { UploadDialog(it, library) }

    // Predictive back on details: the page shrinks and slides with the finger before it closes
    var backProgress by remember { mutableStateOf(0f) }
    androidx.activity.compose.PredictiveBackHandler(enabled = detail != null) { events ->
        try {
            events.collect { backProgress = it.progress }
            library.closeDetail()
        } finally {
            backProgress = 0f
        }
    }
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
            SnackbarHost(snackbar) {
                Snackbar(
                    it, containerColor = shelf.fg, contentColor = shelf.inverse, actionColor = shelf.inverse,
                    shape = androidx.compose.foundation.shape.RoundedCornerShape(8.dp),
                )
            }
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
                header = { Logo(32.dp, Modifier.padding(vertical = 12.dp)) },
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
        val current = detail
        // The last details shown, so they can fade out after closing
        var shownDetail by remember { mutableStateOf(current) }
        if (current != null) shownDetail = current
        val screen: Any = if (current != null) "detail" else tab
        AnimatedContent(
            screen, modifier = Modifier.weight(1f),
            transitionSpec = {
                val toDetail = targetState == "detail"
                val fromDetail = initialState == "detail"
                when {
                    // Details come in from the side a little; going back they leave the same way
                    toDetail -> (fadeIn(tween(220)) + slideInHorizontally(tween(220)) { it / 10 }) togetherWith fadeOut(tween(90))
                    fromDetail -> fadeIn(tween(220, delayMillis = 60)) togetherWith (fadeOut(tween(150)) + slideOutHorizontally(tween(150)) { it / 10 })
                    // Tabs fade through, as Material asks for peer destinations
                    else -> (fadeIn(tween(210, delayMillis = 90)) + scaleIn(tween(210, delayMillis = 90), initialScale = 0.96f)) togetherWith fadeOut(tween(90))
                }
            },
            label = "screen",
        ) { target ->
            val modifier = Modifier.fillMaxSize()
            when (target) {
                "detail" -> shownDetail?.let { d ->
                    val p = if (target == screen) backProgress else 0f
                    DetailScreen(
                        library, d, openBook,
                        modifier.graphicsLayer {
                            val scale = 1f - 0.1f * p
                            scaleX = scale; scaleY = scale
                            translationX = p * 32.dp.toPx()
                            shape = androidx.compose.foundation.shape.RoundedCornerShape((p * 28).dp)
                            clip = p > 0f
                        },
                    )
                }
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
}

package io.github.danielpereod.opdshelf.ui

import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.LocalBook
import io.github.danielpereod.opdshelf.reader.ReaderActions
import io.github.danielpereod.opdshelf.reader.ReaderOverlay
import io.github.danielpereod.opdshelf.reader.ReaderPrefs
import io.github.danielpereod.opdshelf.reader.ReaderUi
import io.github.danielpereod.opdshelf.reader.TextSheet
import io.github.danielpereod.opdshelf.reader.TocItem
import io.github.danielpereod.opdshelf.reader.TocSheet
import kotlinx.coroutines.runBlocking
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.readium.r2.shared.publication.Link
import org.readium.r2.shared.util.Url
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * Renders the app's screens to PNG files, to look at them without a device.
 * Runs only with SCREENSHOT_DIR set; the screens with a catalog also need a
 * running OPDShelf server in SCREENSHOT_SERVER (user dani, password prueba).
 *
 *   SCREENSHOT_DIR=/tmp/shots SCREENSHOT_SERVER=http://127.0.0.1:3000 \
 *     ./gradlew testDebugUnitTest --tests '*ScreenshotTest*'
 */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "es-rES-w400dp-h860dp-xxhdpi")
class ScreenshotTest {
    @get:Rule val compose = createAndroidComposeRule<androidx.activity.ComponentActivity>()

    private val dir = System.getenv("SCREENSHOT_DIR")?.let(::File)
    private val server = System.getenv("SCREENSHOT_SERVER")

    @Before
    fun only() {
        assumeTrue(dir != null)
    }

    private fun shot(name: String) {
        // Lets images arrive and their fade in finish: the clock stands still under Robolectric
        repeat(10) {
            compose.waitForIdle()
            Thread.sleep(250)
            org.robolectric.shadows.ShadowLooper.idleMainLooper(500, java.util.concurrent.TimeUnit.MILLISECONDS)
        }
        compose.waitForIdle()
        dir!!.mkdirs()
        // Drawn in software: PixelCopy, used by captureToImage, never completes under Robolectric
        val view = compose.activity.window.decorView
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        compose.runOnUiThread { view.draw(android.graphics.Canvas(bitmap)) }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun waitFor(text: String) =
        compose.waitUntil(15_000) { compose.onAllNodes(hasText(text, substring = true)).fetchSemanticsNodes().isNotEmpty() }

    private fun click(text: String) = compose.onAllNodes(hasText(text)).onFirst().performClick()

    private fun show(): LibraryViewModel {
        val library = LibraryViewModel(compose.activity.application)
        compose.setContent { AppTheme { Main(library) {} } }
        return library
    }

    private fun configure() {
        assumeTrue(server != null)
        compose.activity.app.settings.apply {
            serverUrl = server!!
            username = "dani"
            password = "prueba"
            deviceName = "Tablet"
        }
    }

    /** Downloads two books from the server, one of them half read. */
    private fun downloadSome() {
        val app = compose.activity.app
        runBlocking {
            val feed = app.api.feed(app.api.url("/opds/books"))
            feed.entries.filter { it.title in setOf("Don Quijote de la Mancha", "Platero y yo") }.forEach { entry ->
                val quijote = entry.title.startsWith("Don Quijote")
                val acquisition = entry.acquisition!!
                val base = entry.id.hashCode().toUInt().toString(16)
                val file = File(app.store.booksDir, "$base.epub")
                app.api.download(acquisition.url, file) {}
                val cover = (entry.coverUrl ?: entry.thumbnailUrl)?.let {
                    File(app.store.booksDir, "$base.cover").also { f -> app.api.download(it, f) {} }.name
                }
                app.store.addBook(
                    LocalBook(
                        id = entry.id, title = entry.title, authors = entry.authors, fileName = file.name,
                        mediaType = acquisition.type, document = base.padEnd(32, '0'), coverFile = cover,
                        summary = entry.summary, format = "EPUB", sizeBytes = file.length(),
                        percentage = if (quijote) 0.42 else 0.0,
                        lastOpenedAt = System.currentTimeMillis() - if (quijote) 3_600_000 else 86_400_000,
                    )
                )
            }
        }
    }

    // -----------------------------------------------------------------------

    @Test
    fun welcome() {
        show()
        shot("01-bienvenida")
        click("Conectar mi servidor")
        shot("02-conectar")
    }

    private fun screens(suffix: String) {
        configure()
        downloadSome()
        val library = show()
        waitFor("Seguir leyendo")
        shot("03-inicio$suffix")
        click("Explorar")
        waitFor("Rimas")
        shot("04-explorar$suffix")
        click("Autores")
        waitFor("Benito Pérez Galdós")
        shot("05-autores$suffix")
        click("Recientes")
        waitFor("Rimas")
        click("Rimas")
        waitFor("Descargar · ")
        shot("06-ficha$suffix")
        compose.runOnUiThread { library.closeDetail() }
        click("Ajustes")
        shot("07-ajustes$suffix")
    }

    @Test
    fun app() = screens("")

    @Test
    @Config(qualifiers = "+night")
    fun appDark() = screens("-oscuro")

    // The same screens on other sizes: a small phone, a tablet both ways, a phone on its side

    @Test
    @Config(qualifiers = "es-rES-w360dp-h640dp-xhdpi")
    fun appSmall() = screens("-pequeno")

    @Test
    @Config(qualifiers = "es-rES-w800dp-h1280dp-xhdpi")
    fun appTablet() = screens("-tablet")

    @Test
    @Config(qualifiers = "es-rES-w1280dp-h800dp-land-xhdpi")
    fun appTabletLandscape() = screens("-tablet-horizontal")

    @Test
    @Config(qualifiers = "es-rES-w860dp-h400dp-land-xxhdpi")
    fun appPhoneLandscape() = screens("-movil-horizontal")

    /** The first run with the largest system font, on a small phone. */
    @Test
    @Config(qualifiers = "es-rES-w360dp-h640dp-xhdpi")
    fun welcomeLargeFont() {
        org.robolectric.RuntimeEnvironment.setFontScale(1.5f)
        show()
        shot("01-bienvenida-letra-grande")
        click("Conectar mi servidor")
        shot("02-conectar-letra-grande")
    }

    /** The app with the system font at 200 %, the largest Android offers. */
    @Test
    @Config(qualifiers = "es-rES-w360dp-h780dp-xxhdpi")
    fun appHugeFont() {
        org.robolectric.RuntimeEnvironment.setFontScale(2f)
        screens("-letra-200")
    }

    @Test
    @Config(qualifiers = "es-rES-w800dp-h1280dp-xhdpi")
    fun welcomeTablet() {
        show()
        shot("01-bienvenida-tablet")
    }

    // -----------------------------------------------------------------------
    // The reader's controls, over a page of text

    private val toc = listOf("Capítulo primero", "Capítulo segundo", "Capítulo tercero", "Capítulo cuarto", "Capítulo quinto")
        .mapIndexed { i, t -> TocItem(t, 0, "c${i + 1}.xhtml", Link(Url("c${i + 1}.xhtml")!!)) }

    private fun readerUi() = ReaderUi().apply {
        title = "Don Quijote de la Mancha"
        chapter = "Capítulo segundo"
        progression = 0.42f
        currentHref = "c2.xhtml"
        this.toc = this@ScreenshotTest.toc
        reflowable = true
        prefs = ReaderPrefs(font = 1)
    }

    private val noActions = object : ReaderActions {
        override fun close() {}
        override fun goToProgression(progression: Double) {}
        override fun goTo(item: TocItem) {}
        override fun changePrefs(prefs: ReaderPrefs) {}
        override fun dismissHint() {}
    }

    @Test
    fun reader() {
        var screen by mutableStateOf(0)
        compose.setContent {
            AppTheme {
                Box(Modifier.fillMaxSize().background(Color(0xFFFFFFFF))) {
                    Column(Modifier.padding(horizontal = 28.dp, vertical = 48.dp)) {
                        Text("Capítulo segundo", fontSize = 24.sp, color = Color(0xFF111111))
                        repeat(6) {
                            Text(
                                "Este es un párrafo del capítulo segundo. La lectura avanza tranquila mientras el texto llena la pantalla con frases de ejemplo que ocupan varias líneas.",
                                fontSize = 17.sp, lineHeight = 26.sp, color = Color(0xFF111111), modifier = Modifier.padding(top = 14.dp),
                            )
                        }
                    }
                    when (screen) {
                        0 -> ReaderOverlay(remember { readerUi().apply { showHint = true } }, noActions)
                        1 -> ReaderOverlay(remember { readerUi().apply { visible = true } }, noActions)
                        else -> Box(Modifier.fillMaxSize().background(Color(0x66000000))) {
                            Column(Modifier.align(Alignment.BottomCenter).background(shelf.pane)) {
                                if (screen == 2) TextSheet(ReaderPrefs(font = 1, spacing = 2)) {} else TocSheet(remember { readerUi() }) {}
                            }
                        }
                    }
                }
            }
        }
        listOf("08-lector-ayuda", "09-lector-controles", "10-lector-texto", "11-lector-indice").forEachIndexed { i, name ->
            screen = i
            shot(name)
        }
    }

    @Test
    @Config(qualifiers = "+night")
    fun readerDark() {
        var screen by mutableStateOf(0)
        compose.setContent {
            AppTheme {
                Box(Modifier.fillMaxSize().background(Color(0xFF0B0B0B))) {
                    Column(Modifier.padding(horizontal = 28.dp, vertical = 48.dp)) {
                        repeat(7) {
                            Text(
                                "Este es un párrafo del capítulo segundo. La lectura avanza tranquila mientras el texto llena la pantalla con frases de ejemplo.",
                                fontSize = 17.sp, lineHeight = 26.sp, color = Color(0xFFDDDDDD), modifier = Modifier.padding(top = 14.dp),
                            )
                        }
                    }
                    if (screen == 0) ReaderOverlay(remember { readerUi().apply { visible = true } }, noActions)
                    else Box(Modifier.fillMaxSize().background(Color(0x66000000))) {
                        Column(Modifier.align(Alignment.BottomCenter).background(shelf.pane)) { TextSheet(ReaderPrefs(theme = 2)) {} }
                    }
                }
            }
        }
        listOf("09-lector-controles-oscuro", "10-lector-texto-oscuro").forEachIndexed { i, name ->
            screen = i
            shot(name)
        }
    }

    /** Reading in sepia on a phone set to light: the bars are sepia too. */
    @Test
    fun readerSepia() {
        compose.setContent {
            AppTheme(readerPalette(1)) {
                Box(Modifier.fillMaxSize().background(readerPalette(1).bg)) {
                    Column(Modifier.padding(horizontal = 28.dp, vertical = 48.dp)) {
                        repeat(7) {
                            Text(
                                "Este es un párrafo del capítulo segundo. La lectura avanza tranquila mientras el texto llena la pantalla con frases de ejemplo.",
                                fontSize = 17.sp, lineHeight = 26.sp, color = Color(0xFF121212), modifier = Modifier.padding(top = 14.dp),
                            )
                        }
                    }
                    ReaderOverlay(remember { readerUi().apply { visible = true; prefs = ReaderPrefs(theme = 1) } }, noActions)
                }
            }
        }
        shot("09-lector-controles-sepia")
    }
}

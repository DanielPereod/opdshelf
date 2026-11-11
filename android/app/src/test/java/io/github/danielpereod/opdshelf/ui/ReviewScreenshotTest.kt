package io.github.danielpereod.opdshelf.ui

import android.graphics.Bitmap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.hasSetTextAction
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.LocalBook
import kotlinx.coroutines.runBlocking
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/** Review captures: every screen across sizes, orientations, themes and font scales. */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "es-rES-w360dp-h780dp-xxhdpi")
class ReviewScreenshotTest {
    @get:Rule val compose = createAndroidComposeRule<androidx.activity.ComponentActivity>()
    private val dir = System.getenv("SCREENSHOT_DIR")?.let(::File)
    private val server = System.getenv("SCREENSHOT_SERVER")

    private fun shot(name: String) {
        repeat(8) {
            compose.waitForIdle(); Thread.sleep(200)
            org.robolectric.shadows.ShadowLooper.idleMainLooper(500, java.util.concurrent.TimeUnit.MILLISECONDS)
        }
        compose.waitForIdle()
        dir!!.mkdirs()
        val view = compose.activity.window.decorView
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        compose.runOnUiThread { view.draw(android.graphics.Canvas(bitmap)) }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }
    private fun waitFor(text: String) =
        compose.waitUntil(15_000) { compose.onAllNodes(hasText(text, substring = true)).fetchSemanticsNodes().isNotEmpty() }
    private fun click(text: String) = compose.onAllNodes(hasText(text)).onFirst().performClick()
    private fun clickDesc(text: String) = compose.onAllNodes(hasContentDescription(text)).onFirst().performClick()
    private fun safe(name: String, block: () -> Unit) = try { block() } catch (e: Throwable) { System.err.println("REVIEW skip $name: $e") }

    private fun show(): LibraryViewModel {
        val library = LibraryViewModel(compose.activity.application)
        compose.setContent { AppTheme { Main(library) {} } }
        return library
    }

    private fun configure() {
        compose.activity.app.settings.apply { serverUrl = server!!; username = "dani"; password = "prueba"; deviceName = "Pixel 8" }
    }

    private fun downloadSome() {
        val app = compose.activity.app
        runBlocking {
            val feed = app.api.feed(app.api.url("/opds/books"))
            feed.entries.filter { it.title in setOf("Don Quijote de la Mancha", "Platero y yo", "Episodios nacionales: Trafalgar, una historia muy larga de título") }.forEach { entry ->
                val quijote = entry.title.startsWith("Don Quijote")
                val acquisition = entry.acquisition!!
                val base = entry.id.hashCode().toUInt().toString(16)
                val file = File(app.store.booksDir, "$base.epub")
                app.api.download(acquisition.url, file) {}
                val cover = (entry.coverUrl ?: entry.thumbnailUrl)?.let { File(app.store.booksDir, "$base.cover").also { f -> app.api.download(it, f) {} }.name }
                app.store.addBook(LocalBook(
                    id = entry.id, title = entry.title, authors = entry.authors, fileName = file.name,
                    mediaType = acquisition.type, document = base.padEnd(32, '0'), coverFile = cover,
                    summary = entry.summary, format = "EPUB", sizeBytes = file.length(),
                    percentage = if (quijote) 0.42 else 0.0,
                    lastOpenedAt = System.currentTimeMillis() - if (quijote) 3_600_000 else 86_400_000,
                ))
            }
        }
    }

    private fun tour(p: String, scale: Float = 1f) {
        assumeTrue(dir != null && server != null)
        RuntimeEnvironment.setFontScale(scale)
        // First run
        show()
        shot("$p-01-bienvenida")
        safe("connect") { click("Conectar mi servidor"); shot("$p-02-conectar") }
        safe("connect-error") {
            compose.onAllNodes(hasSetTextAction()).onFirst().performTextInput("127.0.0.1:1")
            compose.onAllNodes(hasText("Conectar")).fetchSemanticsNodes()
            compose.onAllNodes(hasText("Conectar")).fetchSemanticsNodes().let { compose.onAllNodes(hasText("Conectar"))[it.size - 1].performClick() }; compose.waitUntil(20_000) { compose.onAllNodes(hasText("No se", substring = true)).fetchSemanticsNodes().isNotEmpty() || compose.onAllNodes(hasText("rror", substring = true)).fetchSemanticsNodes().isNotEmpty() }
            shot("$p-02b-conectar-error")
        }
    }

    private fun tourApp(p: String, scale: Float = 1f) {
        assumeTrue(dir != null && server != null)
        RuntimeEnvironment.setFontScale(scale)
        configure(); downloadSome()
        val library = show()
        waitFor("Seguir leyendo"); shot("$p-03-inicio")
        safe("home-sort") { click("Progreso"); shot("$p-03b-inicio-orden") }
        click("Explorar"); waitFor("La Regenta"); shot("$p-04-explorar")
        safe("list") { clickDesc("Lista"); shot("$p-04b-explorar-lista"); clickDesc("Cuadrícula") }
        safe("az") { click("A–Z"); waitFor("Don Quijote"); shot("$p-04c-az") }
        safe("authors") { click("Autores"); waitFor("Benito Pérez Galdós"); shot("$p-05-autores")
            click("Benito Pérez Galdós"); waitFor("Marianela"); shot("$p-05b-autor"); compose.runOnUiThread { library.back() } }
        safe("series") { click("Series"); Thread.sleep(1500); shot("$p-05c-series") }
        safe("folders") { click("Carpetas"); Thread.sleep(1500); shot("$p-05d-carpetas") }
        safe("search") {
            click("Recientes"); waitFor("La Regenta")
            compose.onAllNodes(hasSetTextAction()).onFirst().performTextInput("zzzz")
            compose.runOnUiThread { library.search("zzzz") }
            Thread.sleep(1500); shot("$p-04d-sin-resultados")
            compose.runOnUiThread { library.back() }
        }
        try {
            safe("detail-remote") { click("Recientes"); waitFor("La Regenta"); click("La Regenta"); waitFor("Descargar · "); shot("$p-06-ficha") }
        } finally {
            compose.runOnUiThread { library.closeDetail() }
        }
        try {
            safe("detail-local") { click("Don Quijote de la Mancha"); waitFor("Seguir leyendo ·"); shot("$p-06b-ficha-local")
                click("Quitar del dispositivo"); shot("$p-06c-dialogo-quitar"); click("Cancelar") }
        } finally {
            compose.runOnUiThread { library.closeDetail() }
        }
        click("Ajustes"); shot("$p-07-ajustes")
        safe("change-server") { click("Cambiar de servidor"); shot("$p-07b-ajustes-servidor") }
        safe("disconnect") { click("Mis libros"); click("Ajustes"); click("Desconectar de este servidor"); shot("$p-07c-dialogo-desconectar") }
    }

    @Test fun phoneWelcome() = tour("movil")
    @Test fun phoneApp() = tourApp("movil")
    @Test @Config(qualifiers = "+night") fun phoneDarkApp() = tourApp("movil-oscuro")
    @Test fun phoneFont2Welcome() = tour("movil-fuente200", 2f)
    @Test fun phoneFont2App() = tourApp("movil-fuente200", 2f)
    @Test fun phoneFont13App() = tourApp("movil-fuente130", 1.3f)
    @Test @Config(qualifiers = "es-rES-w780dp-h360dp-land-xxhdpi") fun landWelcome() = tour("horizontal")
    @Test @Config(qualifiers = "es-rES-w780dp-h360dp-land-xxhdpi") fun landApp() = tourApp("horizontal")
    @Test @Config(qualifiers = "es-rES-w840dp-h1280dp-xhdpi") fun tabletWelcome() = tour("tablet")
    @Test @Config(qualifiers = "es-rES-w840dp-h1280dp-xhdpi") fun tabletApp() = tourApp("tablet")
    @Test @Config(qualifiers = "es-rES-w1280dp-h800dp-land-xhdpi") fun tabletLandApp() = tourApp("tablet-horizontal")
    @Test @Config(qualifiers = "es-rES-w1280dp-h800dp-land-xhdpi") fun tabletLandWelcome() = tour("tablet-horizontal")
}

package io.github.danielpereod.opdshelf.ui

import android.graphics.Bitmap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.LocalBook
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * Renders the app's screens to PNG files, to look at them without a device.
 * Runs only with SCREENSHOT_DIR set; the library screens also need a running
 * OPDShelf server in SCREENSHOT_SERVER (user dani, password prueba).
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
        compose.waitForIdle()
        dir!!.mkdirs()
        // Drawn in software: PixelCopy, used by captureToImage, never completes under Robolectric
        val view = compose.activity.window.decorView
        val bitmap = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
        compose.runOnUiThread { view.draw(android.graphics.Canvas(bitmap)) }
        File(dir, "$name.png").outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun show() {
        val library = LibraryViewModel(compose.activity.application)
        compose.setContent { AppTheme { Main(library) {} } }
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

    @Test
    fun settings() {
        show()
        shot("settings")
    }

    @Test
    @Config(qualifiers = "+night")
    fun settingsDark() {
        show()
        shot("settings-dark")
    }

    private fun libraryScreens(suffix: String) {
        configure()
        compose.activity.app.store.addBook(
            LocalBook(
                id = "urn:opdshelf:book:Test.epub", title = "Test Book", authors = listOf("Ana Autora"),
                fileName = "x.epub", mediaType = "application/epub+zip", document = "0".repeat(32), percentage = 0.42,
            )
        )
        show()
        shot("device$suffix")
        compose.onNodeWithText("Biblioteca").performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("Recently added")).fetchSemanticsNodes().isNotEmpty() }
        shot("library-root$suffix")
        compose.onNodeWithText("Recently added").performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("Ana Autora")).fetchSemanticsNodes().isNotEmpty() }
        shot("library-books$suffix")
        compose.onNodeWithContentDescription("Cuadrícula").performClick()
        shot("library-grid$suffix")
    }

    @Test
    fun library() = libraryScreens("")

    @Test
    @Config(qualifiers = "+night")
    fun libraryDark() = libraryScreens("-dark")
}

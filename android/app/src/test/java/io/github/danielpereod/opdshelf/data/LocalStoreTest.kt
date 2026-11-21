package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class LocalStoreTest {
    private fun withDir(block: (File) -> Unit) {
        val dir = kotlin.io.path.createTempDirectory("opdshelf-store").toFile()
        try {
            block(dir)
        } finally {
            dir.deleteRecursively()
        }
    }

    private val book = LocalBook(id = "b", title = "B", fileName = "b.epub", mediaType = "application/epub+zip", document = "doc")

    @Test
    fun aDamagedFileIsKeptAsideInsteadOfBeingTakenForEmpty() = withDir { dir ->
        File(dir, "books.json").writeText("[{\"id\": \"b\", \"tit")
        val store = LocalStore(dir)
        assertTrue(store.books.value.isEmpty())
        // The next write replaces books.json, but the damaged one is still there
        store.addBook(book)
        assertEquals("[{\"id\": \"b\", \"tit", File(dir, "books.json.bad").readText())
        assertEquals(listOf("b"), LocalStore(dir).books.value.map { it.id })
    }

    @Test
    fun changingServerDropsWhatWasWaitingForTheOldOne() = withDir { dir ->
        val store = LocalStore(dir)
        store.addBook(book)
        store.setFinished("b", true, at = 100)
        store.savePosition("doc", LocalPosition(Position(progress = "5", percentage = 0.5), pending = true))
        assertEquals(1, store.pendingMarks().size)
        assertEquals(1, store.pending().size)
        store.discardPending()
        assertTrue(store.pendingMarks().isEmpty())
        assertTrue(store.pending().isEmpty())
        // The books and what they show stay
        assertEquals(0.5, store.position("doc")!!.position.percentage, 0.0)
        assertEquals(1, store.books.value.size)
    }
}

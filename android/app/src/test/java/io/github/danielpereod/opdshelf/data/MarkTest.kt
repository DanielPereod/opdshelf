package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.Rule

class MarkTest {
    @get:Rule val tmp = TemporaryFolder()

    private fun book(id: String = "b1", document: String = "doc1", percentage: Double = 0.3) =
        LocalBook(id = id, title = "T", fileName = "$id.epub", mediaType = "application/epub+zip", document = document, percentage = percentage)

    @Test
    fun finishedBookSendsRead() {
        assertEquals("read", book().copy(finishedByHand = true).markToSend)
    }

    @Test
    fun unmarkingClearsTheMarkUnlessReadToTheEnd() {
        assertNull(book().copy(finishedByHand = false).markToSend)
        assertEquals("unread", book(percentage = 1.0).copy(finishedByHand = false).markToSend)
    }

    @Test
    fun aNewerMarkFromTheServerIsTaken() {
        val marked = book().adopt(Mark("read", 100))
        assertTrue(marked.finished)
        assertEquals(100, marked.finishedAt)
        assertFalse(book(percentage = 1.0).adopt(Mark("unread", 100)).finished)
    }

    @Test
    fun aMarkNotSentYetWins() {
        val mine = book().copy(finishedByHand = true, finishedAt = 50, finishedPending = true)
        assertEquals(mine, mine.adopt(Mark("unread", 200)))
        val sent = mine.copy(finishedPending = false, finishedAt = 300)
        assertEquals(sent, sent.adopt(Mark("unread", 200)))
    }

    @Test
    fun storeQueuesMarksUntilTheServerTakesThem() {
        val store = LocalStore(tmp.root)
        store.addBook(book())
        store.setFinished("b1", true)
        val pending = store.pendingMarks().single()
        assertTrue(pending.finished)

        store.markFinishedSent(pending, 1_000)
        assertTrue(store.pendingMarks().isEmpty())
        assertEquals(1_000, store.book("b1")!!.finishedAt)

        // The web marks it unread later
        store.adoptMarks(mapOf("doc1" to Mark("unread", 2_000)))
        assertFalse(store.book("b1")!!.finished)
        // and it is kept after a restart
        assertFalse(LocalStore(tmp.root).book("b1")!!.finished)
    }

    @Test
    fun aMarkChangedWhileSendingStaysPending() {
        val store = LocalStore(tmp.root)
        store.addBook(book())
        store.setFinished("b1", true)
        val sent = store.pendingMarks().single()
        store.setFinished("b1", false)
        store.markFinishedSent(sent, 1_000)
        assertEquals(false, store.pendingMarks().single().finishedByHand)
    }

    @Test
    fun handMarksFromBeforeSyncAreSentOnce() {
        LocalStore(tmp.root).addBook(book().copy(finishedByHand = true))
        assertEquals(1, LocalStore(tmp.root).pendingMarks().size)
    }
}

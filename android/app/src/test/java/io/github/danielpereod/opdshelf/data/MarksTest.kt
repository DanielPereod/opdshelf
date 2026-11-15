package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class MarksTest {
    private val book = LocalBook(id = "b", title = "B", fileName = "b.epub", mediaType = "application/epub+zip", document = "doc", percentage = 0.5)

    @Test
    fun aNewerMarkFromTheServerIsAdopted() {
        val marked = book.withMark(ReadMark(ReadMark.READ, 100), pending = false)
        assertTrue(marked.finished)
        assertEquals(100, marked.markedAt)
        // Marked as not finished on the web later
        val unmarked = marked.copy(percentage = 1.0).withMark(ReadMark(ReadMark.UNREAD, 200), pending = false)
        assertFalse("a mark wins over how far it was read", unmarked.finished)
        assertEquals(200, unmarked.markedAt)
    }

    @Test
    fun anOlderMarkOrNoneLeavesTheBookAlone() {
        val local = book.copy(finishedByHand = true, markedAt = 300)
        assertSame(local, local.withMark(ReadMark(ReadMark.UNREAD, 200), pending = false))
        assertSame(local, local.withMark(ReadMark(ReadMark.UNREAD, 300), pending = false))
        assertSame(local, local.withMark(null, pending = false))
    }

    @Test
    fun aMarkNotSentYetIsKept() {
        // Marked here offline; the server decides when it gets it
        val local = book.copy(finishedByHand = true, markedAt = 100)
        assertSame(local, local.withMark(ReadMark(ReadMark.UNREAD, 500), pending = true))
    }

    @Test
    fun theLibrarySaysWhereEachBookIsAt() {
        val position = { pct: Double, t: Long -> Position(progress = "/p", percentage = pct, timestamp = t) }
        val library = LibraryProgress(
            books = listOf(
                BookProgress("reading", "Reading.epub", position(0.4, 100)),
                BookProgress("done", "Done.epub", position(0.99, 100)),
                BookProgress("marked", "Marked.epub", position(0.2, 100)),
                // Marked as unread after it was read: back to not started
                BookProgress("reset", "Reset.epub", position(0.7, 100)),
                // Read again after being marked as unread
                BookProgress("again", "Again.epub", position(0.3, 300)),
                BookProgress("gone", null, position(0.5, 100)),
                BookProgress("unstarted", "Unstarted.epub", position(0.0, 100)),
            ),
            marks = listOf(
                MarkEntry("marked", "Marked.epub", ReadMark.READ, 150),
                MarkEntry("reset", "Reset.epub", ReadMark.UNREAD, 200),
                MarkEntry("again", "Again.epub", ReadMark.UNREAD, 200),
                MarkEntry("only", "Only.epub", ReadMark.READ, 50),
            ),
        )
        val byPath = library.byPath()
        assertEquals(RemoteReading(0.4, false), byPath["Reading.epub"])
        assertEquals(true, byPath["Done.epub"]?.finished)
        assertEquals(RemoteReading(1.0, true), byPath["Marked.epub"])
        assertNull(byPath["Reset.epub"])
        assertEquals(RemoteReading(0.3, false), byPath["Again.epub"])
        assertEquals(RemoteReading(1.0, true), byPath["Only.epub"])
        assertNull(byPath["Unstarted.epub"])
        assertEquals(5, byPath.size)
        assertEquals(ReadMark(ReadMark.UNREAD, 200), library.marksByDocument()["reset"])
    }

    @Test
    fun theServersAnswerIsRead() {
        val text = """{"books":[{"document":"d","path":"A.epub","title":"A","current":{"progress":"/p","percentage":0.5,"device":"Kobo","device_id":"k","timestamp":9}}],
            "marks":[{"document":"d","path":"A.epub","title":"A","state":"read","at":10}],"server_time":11}"""
        val library = json.decodeFromString(LibraryProgress.serializer(), text)
        assertEquals(RemoteReading(1.0, true), library.byPath()["A.epub"])
        val progress = json.decodeFromString(
            ProgressResponse.serializer(), """{"document":"d","current":null,"history":[],"mark":{"state":"unread","at":5},"server_time":6}""",
        )
        assertEquals(ReadMark(ReadMark.UNREAD, 5), progress.mark)
    }
}

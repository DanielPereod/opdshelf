package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AnnotationTest {
    private fun a(id: String, updated: Long, note: String = "", percentage: Double = 0.5, device: String = "web") =
        Annotation(id = id, text = "words", note = note, percentage = percentage, created = updated, updated = updated, deviceId = device)

    @Test
    fun localChangesStayOnTopUntilTheServerHasThem() {
        val local = LocalAnnotations(list = listOf(a("h1", 100)), pending = mapOf("h1" to a("h1", 200, "mine", device = "phone")))
        assertEquals("mine", local.visible().single().note)
        // The server still has the old one: the change stays pending
        assertEquals(1, local.adopt(listOf(a("h1", 100))).pending.size)
        // The server has a newer one from elsewhere: it wins
        val adopted = local.adopt(listOf(a("h1", 300, "web")))
        assertTrue(adopted.pending.isEmpty())
        assertEquals("web", adopted.visible().single().note)
    }

    @Test
    fun deletedOnesAreHiddenAndOrderFollowsTheBook() {
        val local = LocalAnnotations(listOf(a("b", 1, percentage = 0.9), a("a", 1, percentage = 0.1), a("c", 1).copy(deleted = true)))
        assertEquals(listOf("a", "b"), local.visible().map { it.id })
    }

    @Test
    fun liveChangesReplaceOlderCopies() {
        val local = LocalAnnotations(listOf(a("h1", 100)))
        val next = local.receive(listOf(a("h1", 50, "older"), a("h2", 120, "new")), deviceId = "phone")
        assertEquals(listOf("", "new"), next.visible().sortedBy { it.id }.map { it.note })
    }

    @Test
    fun sentChangesLeaveTheQueue() {
        val change = a("h1", 200, "x", device = "phone")
        val local = LocalAnnotations(pending = mapOf("h1" to change))
        assertTrue(local.sent(listOf(change)).pending.isEmpty())
        assertEquals(1, local.sent(listOf(change.copy(note = "other"))).pending.size)
    }

    @Test
    fun markdownGroupsByChapter() {
        val md = annotationsMarkdown(
            "Book", "Ann",
            listOf(a("1", 1).copy(chapter = "One"), a("2", 1, "Note").copy(chapter = "One"), a("3", 1).copy(chapter = "")),
            "No chapter",
        )
        assertEquals("# Book\n\n*Ann*\n\n## One\n\n> words\n\n> words\n\nNote\n\n## No chapter\n\n> words\n", md)
    }
}

package io.github.danielpereod.opdshelf.reader

import org.junit.Assert.assertEquals
import org.junit.Test

class TocIndexTest {
    private val order = listOf("cover.html", "ch1.html", "ch1_split_001.html", "ch2.html", "ch2_split_001.html", "notes.html")
    private val toc = listOf("ch1.html", "ch2.html#start")

    @Test
    fun aFileInTheContentsIsItsEntry() {
        assertEquals(0, currentTocIndex(toc, order, "ch1.html"))
        assertEquals(1, currentTocIndex(toc, order, "ch2.html"))
    }

    @Test
    fun aSplitFileBelongsToTheEntryBeforeIt() {
        // Next chapter from here is entry 1, not the start of the book
        assertEquals(0, currentTocIndex(toc, order, "ch1_split_001.html"))
        assertEquals(1, currentTocIndex(toc, order, "ch2_split_001.html#x"))
        assertEquals(1, currentTocIndex(toc, order, "notes.html"))
    }

    @Test
    fun beforeTheFirstEntryThereIsNone() {
        assertEquals(-1, currentTocIndex(toc, order, "cover.html"))
        assertEquals(-1, currentTocIndex(toc, order, null))
    }
}

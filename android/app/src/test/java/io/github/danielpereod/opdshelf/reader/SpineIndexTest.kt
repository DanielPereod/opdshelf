package io.github.danielpereod.opdshelf.reader

import org.junit.Assert.assertEquals
import org.junit.Test

class SpineIndexTest {
    @Test
    fun readsTheChapterOfAnXPointer() {
        assertEquals(2, spineIndex("/body/DocFragment[3]/body/div[1]/p[12]/text()[1].45"))
        assertEquals(0, spineIndex("/body/DocFragment/body/p"))
        assertEquals(-1, spineIndex("12"))
    }
}

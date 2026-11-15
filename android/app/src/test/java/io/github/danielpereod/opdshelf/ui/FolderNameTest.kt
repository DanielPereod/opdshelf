package io.github.danielpereod.opdshelf.ui

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class FolderNameTest {
    @Test
    fun oneFolderAtATime() {
        assertTrue(isFolderName("Novela negra"))
        assertTrue(isFolderName("  Poesía  "))
        assertFalse(isFolderName(""))
        assertFalse(isFolderName("   "))
        assertFalse(isFolderName("A/B"))
        assertFalse(isFolderName("A\\B"))
        assertFalse(isFolderName(".."))
        assertFalse(isFolderName("."))
    }
}

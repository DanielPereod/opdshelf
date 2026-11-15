package io.github.danielpereod.opdshelf.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SelectionTest {
    @Test
    fun selectAllTurnsIntoDeselectAll() {
        val shown = listOf("a", "b", "c")
        val some = listOf("b")
        assertFalse(allPicked(some, shown))
        val all = toggleAll(some, shown)
        assertEquals(listOf("b", "a", "c"), all)
        assertTrue(allPicked(all, shown))
        // Pressed again: nothing shown stays picked, which ends selecting
        assertEquals(emptyList<String>(), toggleAll(all, shown))
        // Books picked before a search narrowed the list stay picked
        assertEquals(listOf("z"), toggleAll(listOf("z", "a", "b", "c"), shown))
        assertFalse(allPicked(emptyList(), emptyList()))
    }

    @Test
    fun placeholdersShowTheTitleWhereItFits() {
        // A grid's tile: the title in three readable lines, the format as a tag
        val tile = placeholderText(104f, large = false)
        assertEquals(12, tile.titleSp)
        assertEquals(3, tile.titleLines)
        assertTrue(tile.tagSp in 9..11)
        // Large tiles and the details' cover, as before
        assertEquals(PlaceholderText(15, 20, 4, 11), placeholderText(88f, large = true))
        assertEquals(15, placeholderText(160f, large = false).titleSp)
        // A list's small cover: only the format, readable, the title is beside it
        val row = placeholderText(48f, large = false)
        assertNull(row.titleSp)
        assertTrue(row.tagSp >= 10)
    }
}

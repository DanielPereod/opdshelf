package io.github.danielpereod.opdshelf.ui

import io.github.danielpereod.opdshelf.data.friendlyDeviceName
import io.github.danielpereod.opdshelf.data.publishedYear
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LayoutTest {
    @Test
    fun gridsUseLargerTilesOnTablets() {
        // A phone keeps its tiles; a tablet gets real covers
        assertEquals(96f, gridMinCellDp(400, 1f))
        assertEquals(140f, gridMinCellDp(720, 1f))
        assertEquals(140f * 1.5f * 0.75f, gridMinCellDp(720, 1.5f), 0.01f)
        // A 720 dp tablet in portrait (rail of 80 dp, 24 dp margins, 16 dp apart): 3 columns, not 4 small ones
        assertEquals(3, gridColumns(720f - 80f - 48f, gridMinCellDp(720, 1f), 16f))
        // On its side, 1152 dp: 6
        assertEquals(6, gridColumns(1152f - 80f - 48f, gridMinCellDp(1152, 1f), 16f))
        // A phone 400 dp wide, 16 dp margins, 12 dp apart: 3 as before
        assertEquals(3, gridColumns(400f - 32f, gridMinCellDp(400, 1f), 12f))
        assertEquals(1, gridColumns(50f, 96f, 12f))
    }

    @Test
    fun placeholdersAreEvenAndSayWhatTheCaptionDoesNot() {
        assertEquals(3, placeholderText(150f, large = false).titleLines)
        assertEquals(4, placeholderText(150f, large = true).titleLines)
        assertNull(placeholderText(50f, large = false).titleSp)
        assertEquals("Viktor Frankl", placeholderLine("El hombre en busca de sentido", listOf("Viktor Frankl")))
        assertEquals("EH", placeholderLine("El hombre en busca de sentido", emptyList()))
        assertEquals("?", placeholderLine("", emptyList()))
    }

    @Test
    fun onlyRealPublicationYears() {
        assertNull(publishedYear("0101-01-01T00:00:00+00:00"))
        assertNull(publishedYear("9999", thisYear = 2026))
        assertNull(publishedYear(null))
        assertEquals("2019", publishedYear("2019-05-01"))
        assertEquals("2027", publishedYear("2027", thisYear = 2026))
    }

    @Test
    fun devicesGoByTheirGivenName() {
        assertEquals("Xiaomi Pad 6", friendlyDeviceName("Xiaomi Pad 6", null, "Xiaomi", "23043RP34G"))
        assertEquals("Dani's tablet", friendlyDeviceName(null, "Dani's tablet", "Xiaomi", "23043RP34G"))
        // A name that is only the model's code says no more than the model
        assertEquals("Xiaomi 23043RP34G", friendlyDeviceName("23043RP34G", " ", "Xiaomi", "23043RP34G"))
        assertEquals("Google Pixel 8", friendlyDeviceName(null, null, "Google", "Pixel 8"))
        assertEquals("Samsung SM-X200", friendlyDeviceName(null, null, "samsung", "SM-X200"))
    }
}

package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PositionTest {
    private val me = "phone"
    private fun pos(progress: String, timestamp: Long, device: String = "Kobo", id: String = "kobo") =
        Position(progress = progress, device = device, deviceId = id, timestamp = timestamp)

    @Test
    fun nothingSavedAnywhere() {
        val start = chooseStart(null, null, me)
        assertNull(start.position)
        assertNull(start.fromDevice)
    }

    @Test
    fun serverPositionFromAnotherDeviceIsAnnounced() {
        val server = pos("/body/DocFragment[2]/body/p[3]/text()[1].0", 200)
        val local = LocalPosition(pos("/body/DocFragment[1]/body/p[1]", 100, "Phone", me), pending = false, locator = "{}")
        val start = chooseStart(local, server, me)
        assertEquals(server, start.position)
        assertEquals("Kobo", start.fromDevice)
        // The saved locator is for another point, so it is not used
        assertNull(start.locator)
    }

    @Test
    fun samePointKeepsTheExactLocatorAndSaysNothing() {
        val server = pos("/body/DocFragment[2]/body/p[3]", 200)
        val local = LocalPosition(server.copy(timestamp = 150), pending = false, locator = "{\"href\":\"c2.xhtml\"}")
        val start = chooseStart(local, server, me)
        assertEquals("{\"href\":\"c2.xhtml\"}", start.locator)
        assertNull(start.fromDevice)
    }

    @Test
    fun newerOfflineReadingWins() {
        val server = pos("/body/DocFragment[2]/body/p[3]", 200)
        val local = LocalPosition(pos("/body/DocFragment[3]/body/p[1]", 300, "Phone", me), pending = true, locator = "L")
        val start = chooseStart(local, server, me)
        assertEquals(local.position, start.position)
        assertEquals("L", start.locator)
    }

    @Test
    fun olderOfflineReadingLosesToNewerReadingElsewhere() {
        val server = pos("/body/DocFragment[2]/body/p[3]", 500)
        val local = LocalPosition(pos("/body/DocFragment[1]/body/p[1]", 300, "Phone", me), pending = true)
        assertEquals(server, chooseStart(local, server, me).position)
    }

    @Test
    fun serverUrlsAreNormalized() {
        assertEquals("http://192.168.0.204:3080", normalizeServerUrl(" 192.168.0.204:3080/ "))
        assertEquals("https://books.example/shelf", normalizeServerUrl("https://books.example/shelf/"))
        assertEquals("https://books.example", normalizeServerUrl("books.example"))
        assertEquals("http://nas.local:3000", normalizeServerUrl("nas.local:3000"))
    }
}

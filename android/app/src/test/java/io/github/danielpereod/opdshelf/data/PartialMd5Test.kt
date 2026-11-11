package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class PartialMd5Test {
    @get:Rule val tmp = TemporaryFolder()

    @Test
    fun smallFileMatchesTheServer() {
        // The id the server and KOReader give a file containing "book one" (see tests/sync.rs)
        val file = tmp.newFile("One.epub").apply { writeBytes("book one".toByteArray()) }
        assertEquals("502000418ae95849b554660b1ee1d388", partialMd5(file))
    }

    @Test
    fun samplesAcrossALargerFile() {
        val file = tmp.newFile("big.bin").apply {
            writeBytes(ByteArray(300_000) { i -> ((i * 7 + 3) % 251).toByte() })
        }
        // Worked out independently with KOReader's algorithm
        assertEquals("3e53b613def422b326c240820288f5ef", partialMd5(file))
    }
}

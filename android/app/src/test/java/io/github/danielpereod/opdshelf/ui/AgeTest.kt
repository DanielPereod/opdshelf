package io.github.danielpereod.opdshelf.ui

import org.junit.Assert.assertEquals
import org.junit.Test

class AgeTest {
    @Test
    fun neverZeroMinutesAgo() {
        val now = 1_000_000_000_000L
        assertEquals(Age.JustNow, ageOf(now - 59_000, now))
        // A clock a little ahead is not "in 1 minute"
        assertEquals(Age.JustNow, ageOf(now + 30_000, now))
        assertEquals(Age.Minutes(1), ageOf(now - 60_000, now))
        assertEquals(Age.Minutes(4), ageOf(now - 4 * 60_000 - 5_000, now))
        assertEquals(Age.Hours(2), ageOf(now - 2 * 3_600_000, now))
        assertEquals(Age.Days(3), ageOf(now - 3 * 86_400_000, now))
        assertEquals(Age.Older, ageOf(now - 8 * 86_400_000L, now))
    }
}

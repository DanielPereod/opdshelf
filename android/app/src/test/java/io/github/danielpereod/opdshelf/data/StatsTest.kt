package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StatsTest {
    @Test
    fun durationsAreWrittenAsOnTheWeb() {
        assertEquals(ReadingTime(0, 0), readingTime(0))
        // Whole minutes read, as the streak counts a day from its first full minute
        assertEquals(ReadingTime(0, 0), readingTime(33))
        assertEquals(ReadingTime(0, 1), readingTime(60))
        assertEquals(ReadingTime(0, 1), readingTime(119))
        assertEquals(ReadingTime(0, 40), readingTime(40 * 60 + 10))
        assertEquals(ReadingTime(0, 59), readingTime(59 * 60))
        assertEquals(ReadingTime(2, 10), readingTime(130 * 60))
        // "13 h", never "13 h 0 min"
        assertEquals(ReadingTime(13, 0), readingTime(13 * 3600 + 20))
        assertEquals(ReadingTime(0, 0), readingTime(-5))
    }

    @Test
    fun theServersStatisticsAreRead() {
        val text = """{"today":{"seconds":2400,"sessions":2},"total_seconds":90000,"reading_days":40,
            "streak":{"current":12,"longest":30},
            "days":[{"date":"2026-11-13","seconds":33},{"date":"2026-11-14","seconds":0},{"date":"2026-11-15","seconds":2400}],
            "top_books":[{"document":"d","title":null,"seconds":2400}],
            "finished":[{"month":"2025-12","count":1,"books":[]},{"month":"2026-01","count":2,"books":[{"document":"d","title":"T","at":5}]},{"month":"2026-11","count":1,"books":[]}]}"""
        val stats = json.decodeFromString(ReadingStats.serializer(), text)
        assertEquals(2400, stats.today.seconds)
        assertEquals(12, stats.streak.current)
        assertEquals(2433, stats.last30Seconds)
        assertEquals(1, stats.readingDaysIn30)
        assertEquals(3, stats.finishedIn("2026"))
        assertEquals(1, stats.finishedIn("2025"))
        assertFalse(stats.isEmpty)
        assertEquals(null, stats.topBooks.single().title)
        assertTrue(ReadingStats().isEmpty)
        // Kept on disk as it came
        val cached = CachedStats(stats, 1234)
        assertEquals(cached, json.decodeFromString(CachedStats.serializer(), json.encodeToString(CachedStats.serializer(), cached)))
    }
}

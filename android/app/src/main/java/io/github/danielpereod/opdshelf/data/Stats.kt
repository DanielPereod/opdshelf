package io.github.danielpereod.opdshelf.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlin.math.roundToLong

/** Reading statistics as the server works them out (`GET /api/stats`), from every device's reading. */
@Serializable
data class ReadingStats(
    val today: TodayStats = TodayStats(),
    @SerialName("total_seconds") val totalSeconds: Long = 0,
    @SerialName("reading_days") val readingDays: Int = 0,
    val streak: Streak = Streak(),
    /** The last 30 days, oldest first, the ones without reading included. */
    val days: List<DayStats> = emptyList(),
    /** The most read books of the last 30 days, most read first. */
    @SerialName("top_books") val topBooks: List<TopBook> = emptyList(),
    /** Books finished in each of the last 12 months, oldest first. */
    val finished: List<MonthStats> = emptyList(),
) {
    /** Nothing read on any device yet. */
    val isEmpty: Boolean get() = totalSeconds <= 0 && finished.all { it.count == 0 }

    val last30Seconds: Long get() = days.sumOf { it.seconds }

    val readingDaysIn30: Int get() = days.count { it.seconds > 0 }

    /** Books finished in [year] (e.g. "2026"), as far as the last 12 months go. */
    fun finishedIn(year: String): Int = finished.filter { it.month.startsWith("$year-") }.sumOf { it.count }
}

@Serializable
data class TodayStats(val seconds: Long = 0, val sessions: Int = 0)

@Serializable
data class Streak(val current: Int = 0, val longest: Int = 0)

/** [date] is YYYY-MM-DD. */
@Serializable
data class DayStats(val date: String = "", val seconds: Long = 0)

@Serializable
data class TopBook(val document: String = "", val title: String? = null, val seconds: Long = 0)

/** [month] is YYYY-MM. */
@Serializable
data class MonthStats(val month: String = "", val count: Int = 0, val books: List<FinishedBook> = emptyList())

@Serializable
data class FinishedBook(val document: String = "", val title: String? = null, val at: Long = 0)

/** The last statistics fetched, kept to show them offline; [fetchedAt] in milliseconds. */
@Serializable
data class CachedStats(val stats: ReadingStats, val fetchedAt: Long)

/**
 * A duration as the web panel writes it: whole minutes under an hour, then
 * hours and minutes, without "0 min" ("13 h", never "13 h 0 min").
 */
data class ReadingTime(val hours: Long, val minutes: Long)

fun readingTime(seconds: Long): ReadingTime {
    val minutes = (seconds.coerceAtLeast(0) / 60.0).roundToLong()
    return if (minutes < 60) ReadingTime(0, minutes) else ReadingTime(minutes / 60, minutes % 60)
}

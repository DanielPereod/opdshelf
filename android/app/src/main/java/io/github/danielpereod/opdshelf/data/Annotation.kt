package io.github.danielpereod.opdshelf.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * A highlight, with or without a note, as OPDShelf stores it and shares it
 * with the web reader and KOReader. `pos0` and `pos1` are KOReader xpointers
 * of its start and end (page numbers for PDFs).
 */
@Serializable
data class Annotation(
    val id: String,
    val text: String = "",
    val note: String = "",
    val color: String = "yellow",
    val style: String = "lighten",
    val pos0: String = "",
    val pos1: String = "",
    val cfi: String? = null,
    val context: PositionContext? = null,
    val chapter: String = "",
    val page: Int? = null,
    val percentage: Double = 0.0,
    val created: Long = 0,
    val updated: Long = 0,
    val deleted: Boolean = false,
    val device: String = "",
    @SerialName("device_id") val deviceId: String = "",
) {
    val hasNote: Boolean get() = note.isNotBlank()

    companion object {
        /** The colours offered when highlighting; KOReader can use the others too. */
        val PICK = listOf("yellow", "green", "blue", "red", "purple")

        /** Colours as ARGB, as the web shows them. */
        val COLORS = mapOf(
            "yellow" to 0xFFF5C400.toInt(), "orange" to 0xFFFF9800.toInt(), "red" to 0xFFEF5350.toInt(),
            "purple" to 0xFFAB47BC.toInt(), "blue" to 0xFF42A5F5.toInt(), "cyan" to 0xFF26C6DA.toInt(),
            "green" to 0xFF66BB6A.toInt(), "olive" to 0xFF9E9D24.toInt(), "gray" to 0xFF9E9E9E.toInt(),
        )

        fun newId(): String = "a" + java.util.UUID.randomUUID().toString().replace("-", "").take(24)

        fun argb(color: String): Int = COLORS[color] ?: COLORS.getValue("yellow")
    }
}

@Serializable
data class AnnotationsResponse(
    val document: String = "",
    val annotations: List<Annotation> = emptyList(),
)

/** A book's highlights on this device: the server's copy and the changes not sent yet. */
@Serializable
data class LocalAnnotations(
    val list: List<Annotation> = emptyList(),
    val pending: Map<String, Annotation> = emptyMap(),
) {
    /** What the reader sees: the server's copy with local changes on top, in reading order. */
    fun visible(): List<Annotation> =
        (list.associateBy { it.id } + pending).values.filterNot { it.deleted }.sortedWith(readingOrder)

    fun find(id: String): Annotation? = pending[id] ?: list.firstOrNull { it.id == id }

    /** Takes the server's list, keeping local changes it does not have yet. */
    fun adopt(server: List<Annotation>): LocalAnnotations {
        val byId = server.associateBy { it.id }
        return LocalAnnotations(server, pending.filterValues { mine -> (byId[mine.id]?.updated ?: Long.MIN_VALUE) <= mine.updated })
    }

    /** Changes that arrived from elsewhere: each one replaces an older copy. */
    fun receive(changed: List<Annotation>, deviceId: String): LocalAnnotations {
        val map = list.associateBy { it.id }.toMutableMap()
        val left = pending.toMutableMap()
        for (a in changed) {
            val kept = map[a.id]
            if (kept != null && kept.updated > a.updated) continue
            map[a.id] = a
            val mine = left[a.id]
            if (mine != null && mine.updated <= a.updated && a.deviceId != deviceId) left.remove(a.id)
        }
        return LocalAnnotations(map.values.toList(), left)
    }

    /** Forgets the pending changes the server took, unless they changed again meanwhile. */
    fun sent(batch: List<Annotation>): LocalAnnotations =
        copy(pending = pending.filter { (id, a) -> batch.none { it.id == id && it == a } })

    companion object {
        val readingOrder = compareBy<Annotation>({ it.percentage }, { it.created }, { it.id })
    }
}

/** The highlights of a book as Markdown, by chapter. */
fun annotationsMarkdown(title: String, authors: String, list: List<Annotation>, noChapter: String): String = buildString {
    append("# ").append(title.trim()).append('\n')
    if (authors.isNotBlank()) append("\n*").append(authors.trim()).append("*\n")
    var chapter: String? = null
    for (a in list.filterNot { it.deleted }) {
        if (a.chapter != chapter) {
            chapter = a.chapter
            append("\n## ").append(a.chapter.ifBlank { noChapter }).append('\n')
        }
        append('\n')
        a.text.trim().lines().forEach { append("> ").append(it.trim()).append('\n') }
        if (a.hasNote) append('\n').append(a.note.trim()).append('\n')
    }
}

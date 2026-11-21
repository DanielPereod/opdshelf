package io.github.danielpereod.opdshelf.data

import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

import org.w3c.dom.Element
import java.io.InputStream
import javax.xml.parsers.DocumentBuilderFactory

/** An OPDS 1 feed: folders and lists to browse, or books. */
data class Feed(
    val title: String,
    val entries: List<Entry>,
    val nextUrl: String?,
    val searchUrl: String?,
)

data class Entry(
    val id: String,
    val title: String,
    val authors: List<String>,
    val summary: String?,
    /** A feed to open, for navigation entries */
    val feedUrl: String?,
    val acquisition: Acquisition?,
    val thumbnailUrl: String?,
    val coverUrl: String?,
    val meta: BookMeta = BookMeta(),
    /** The book's id for syncing (a partial MD5 of its file), as the server says it; null if it does not. */
    val document: String? = null,
)

/**
 * The downloaded book of an entry: the one under its id or, when the book was moved or
 * renamed on the server (its id is its path), the one with the same document id.
 */
fun List<LocalBook>.localFor(entry: Entry): LocalBook? =
    firstOrNull { it.id == entry.id } ?: entry.document?.let { document -> firstOrNull { it.document == document } }

/** What else the catalog says about a book, shown in its details. */
@kotlinx.serialization.Serializable
data class BookMeta(
    val publisher: String? = null,
    /** As given, usually a year or an ISO date. */
    val issued: String? = null,
    /** An ISO 639 code, such as "es". */
    val language: String? = null,
    val series: String? = null,
    val seriesIndex: String? = null,
    val subjects: List<String> = emptyList(),
) {
    val isEmpty get() = publisher == null && issued == null && language == null && series == null && subjects.isEmpty()
}

/**
 * The year a book was published, from the catalog's date ("2019-05-01",
 * "2019"), or null when it is not a real one: Calibre writes 0101-01-01 for
 * "unknown", and some books say year 0 or one far ahead.
 */
fun publishedYear(issued: String?, thisYear: Int = java.util.Calendar.getInstance().get(java.util.Calendar.YEAR)): String? {
    val year = Regex("""(?<!\d)(\d{4})(?!\d)""").find(issued ?: return null)?.groupValues?.get(1) ?: return null
    return year.takeIf { it.toInt() in 1000..thisYear + 1 }
}

data class Acquisition(val url: String, val type: String, val length: Long?, val format: String?)

private const val ACQUISITION = "http://opds-spec.org/acquisition"

fun parseFeed(input: InputStream, baseUrl: String): Feed {
    val factory = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
    val doc = factory.newDocumentBuilder().parse(input)
    val root = doc.documentElement
    val base = baseUrl.toHttpUrlOrNull()
    val resolve = { href: String -> resolveHref(base, href) }

    val feedLinks = root.children("link")
    val next = feedLinks.firstOrNull { it.getAttribute("rel") == "next" }?.getAttribute("href")?.let(resolve)
    val search = feedLinks.firstOrNull {
        it.getAttribute("rel") == "search" && it.getAttribute("type").contains("atom+xml")
    }?.getAttribute("href")?.let(resolve)

    val entries = root.children("entry").map { entry ->
        val links = entry.children("link")
        val acquisition = links.firstOrNull { it.getAttribute("rel").startsWith(ACQUISITION) }?.let {
            Acquisition(
                url = resolve(it.getAttribute("href")),
                type = it.getAttribute("type"),
                length = it.getAttribute("length").toLongOrNull(),
                format = it.getAttribute("title").ifEmpty { null },
            )
        }
        val feed = links.firstOrNull {
            it.getAttribute("type").contains("profile=opds-catalog") || it.getAttribute("rel") == "subsection"
        }?.getAttribute("href")?.let(resolve)
        val title = entry.childText("title") ?: ""
        Entry(
            id = entry.childText("id") ?: "",
            title = if (acquisition == null) folderTitle(title) else title,
            authors = entry.children("author").mapNotNull { it.childText("name") },
            summary = entry.childText("summary") ?: entry.childText("content"),
            feedUrl = if (acquisition == null) feed else null,
            acquisition = acquisition,
            thumbnailUrl = links.firstOrNull { it.getAttribute("rel") == "http://opds-spec.org/image/thumbnail" }
                ?.getAttribute("href")?.let(resolve),
            coverUrl = links.firstOrNull { it.getAttribute("rel") == "http://opds-spec.org/image" }
                ?.getAttribute("href")?.let(resolve),
            document = entry.childText("document"),
            meta = BookMeta(
                publisher = entry.childText("publisher"),
                issued = entry.childText("issued") ?: entry.childText("published"),
                language = entry.childText("language"),
                series = entry.childText("series"),
                seriesIndex = entry.childText("series_index"),
                subjects = entry.children("category").mapNotNull {
                    (it.getAttribute("label").ifEmpty { null } ?: it.getAttribute("term").ifEmpty { null })?.trim()
                }.distinct(),
            ),
        )
    }
    return Feed(root.childText("title") ?: "", entries, next, search)
}

/**
 * The server puts a folder emoji before folder names for readers that show no
 * icons; the app draws its own, so it goes.
 */
internal fun folderTitle(title: String): String =
    if (title.startsWith("\uD83D\uDCC1")) title.removePrefix("\uD83D\uDCC1").trimStart() else title

private fun Element.children(name: String): List<Element> {
    val result = mutableListOf<Element>()
    val nodes = childNodes
    for (i in 0 until nodes.length) {
        val node = nodes.item(i)
        if (node is Element && (node.localName ?: node.nodeName) == name) result += node
    }
    return result
}

private fun Element.childText(name: String): String? =
    children(name).firstOrNull()?.textContent?.trim()?.ifEmpty { null }

/**
 * Resolves a link of the feed as RFC 3986 says. The braces of search templates
 * such as `{searchTerms}` are kept as they are.
 */
internal fun resolveHref(base: HttpUrl?, href: String): String {
    val escaped = href.trim().replace(" ", "%20").replace("{", "%7B").replace("}", "%7D")
    val resolved = base?.resolve(escaped)?.toString() ?: escaped
    return resolved.replace("%7B", "{").replace("%7D", "}")
}

/** The book's path in the library, from its download link: "Comics/Saga 1.cbz". */
val Entry.serverPath: String?
    get() {
        val segments = acquisition?.url?.toHttpUrlOrNull()?.pathSegments ?: return null
        val at = segments.indexOf("download")
        return if (at < 0 || at == segments.lastIndex) null else segments.drop(at + 1).joinToString("/")
    }

/** Percent-encodes each segment of a library path as the server does, keeping the slashes. */
fun encodeLibraryPath(path: String): String = path.split('/').joinToString("/") { segment ->
    buildString {
        segment.toByteArray(Charsets.UTF_8).forEach { byte ->
            val c = byte.toInt() and 0xff
            if (c in 'a'.code..'z'.code || c in 'A'.code..'Z'.code || c in '0'.code..'9'.code || c.toChar() in "-._~") append(c.toChar())
            else append("%%%02X".format(c))
        }
    }
}

/** The entry of the same book after the server moved it from [oldPath] to [newPath]: its id and links follow the path. */
fun Entry.movedTo(oldPath: String, newPath: String): Entry {
    val from = encodeLibraryPath(oldPath)
    val to = encodeLibraryPath(newPath)
    val swap = { url: String -> url.replace("/$from", "/$to") }
    return copy(
        id = id.replace(from, to),
        acquisition = acquisition?.copy(url = swap(acquisition.url)),
        thumbnailUrl = thumbnailUrl?.let(swap),
        coverUrl = coverUrl?.let(swap),
    )
}

/** The path in the library of a folder of the Folders section ("Comics/Marvel"), null for other entries. */
val Entry.folderPath: String?
    get() {
        if (acquisition != null) return null
        val segments = feedUrl?.toHttpUrlOrNull()?.pathSegments ?: return null
        val at = segments.indexOf("folders")
        if (at < 1 || segments[at - 1] != "opds" || at == segments.lastIndex) return null
        return segments.drop(at + 1).filter { it.isNotEmpty() }.joinToString("/").ifEmpty { null }
    }

/** How many books a folder holds, subfolders included, from its summary ("3 books", "3 libros"). */
val Entry.folderBookCount: Int?
    get() = summary?.let { Regex("""^\s*(\d+)""").find(it) }?.groupValues?.get(1)?.toIntOrNull()

private const val BOOK_ID = "urn:opdshelf:book:"

/**
 * The id of a book inside the folder [from] once the folder is moved to [to],
 * or null when the book is not inside it. Ids follow the book's path.
 */
fun bookIdAfterFolderMove(id: String, from: String, to: String): String? {
    val prefix = BOOK_ID + encodeLibraryPath(from) + "/"
    if (!id.startsWith(prefix)) return null
    return BOOK_ID + encodeLibraryPath(to) + "/" + id.removePrefix(prefix)
}

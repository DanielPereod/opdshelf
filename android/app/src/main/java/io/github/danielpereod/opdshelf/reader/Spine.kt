package io.github.danielpereod.opdshelf.reader

import java.io.File
import java.util.zip.ZipFile
import javax.xml.parsers.DocumentBuilderFactory
import org.w3c.dom.Element

/**
 * Converts between Readium's reading order and the EPUB spine.
 *
 * KOReader numbers chapters in xpointers (`DocFragment[N]`) by their place in
 * the spine, and so does the web reader. Readium leaves the items marked
 * `linear="no"` out of its reading order (often the cover), so its indexes are
 * shifted from that item on and positions would land in another chapter.
 *
 * `spineOf[i]` is the spine index of the reading order's item `i`.
 */
class Spine(private val spineOf: List<Int>) {
    /** Spine index of a reading order item, for `DocFragment[N]`. */
    fun spineIndex(readingOrderIndex: Int): Int = spineOf.getOrElse(readingOrderIndex) { readingOrderIndex }

    /**
     * Reading order index of a spine item. A non-linear item Readium cannot show
     * leads to the next one it can, or -1 when there is none.
     */
    fun readingOrderIndex(spineIndex: Int): Int {
        if (spineIndex < 0) return -1
        if (spineOf.isEmpty()) return spineIndex
        return spineOf.indexOfFirst { it >= spineIndex }
    }

    companion object {
        /** Reading order and spine are the same. */
        val SAME = Spine(emptyList())

        /** Reads the spine of an EPUB file and matches it with Readium's reading order hrefs. */
        fun of(epub: File, readingOrder: List<String>): Spine = runCatching {
            ZipFile(epub).use { zip ->
                fun read(path: String) = zip.getEntry(path)?.let { e -> zip.getInputStream(e).use { it.readBytes() } }
                val container = read("META-INF/container.xml") ?: return SAME
                val opfPath = rootfile(container) ?: return SAME
                val opf = read(opfPath) ?: return SAME
                match(spineHrefs(opf, opfPath), readingOrder)
            }
        }.getOrDefault(SAME)

        /** Path of the package document inside the EPUB. */
        fun rootfile(container: ByteArray): String? {
            val doc = parse(container)
            val rootfiles = doc.getElementsByTagNameNS("*", "rootfile")
            for (i in 0 until rootfiles.length) {
                val path = (rootfiles.item(i) as Element).getAttribute("full-path")
                if (path.isNotEmpty()) return normalize(path)
            }
            return null
        }

        /** Hrefs of every spine item, in order and linear or not, as paths from the root of the EPUB. */
        fun spineHrefs(opf: ByteArray, opfPath: String): List<String> {
            val doc = parse(opf)
            val base = opfPath.substringBeforeLast('/', "")
            val manifest = HashMap<String, String>()
            val items = doc.getElementsByTagNameNS("*", "item")
            for (i in 0 until items.length) {
                val item = items.item(i) as Element
                manifest[item.getAttribute("id")] = item.getAttribute("href")
            }
            val itemrefs = doc.getElementsByTagNameNS("*", "itemref")
            return (0 until itemrefs.length).mapNotNull { i ->
                manifest[(itemrefs.item(i) as Element).getAttribute("idref")]?.let { resolve(base, it) }
            }
        }

        /** Matches reading order hrefs with spine hrefs. Anything unexpected keeps them the same. */
        fun match(spine: List<String>, readingOrder: List<String>): Spine {
            val index = spine.withIndex().associate { (i, href) -> href to i }
            val spineOf = readingOrder.map { index[normalize(it)] ?: return SAME }
            if (spineOf.zipWithNext().any { (a, b) -> b <= a }) return SAME
            return Spine(spineOf)
        }

        private fun parse(xml: ByteArray) = DocumentBuilderFactory.newInstance().apply {
            isNamespaceAware = true
            isExpandEntityReferences = false
            runCatching { setFeature("http://apache.org/xml/features/nonvalidating/load-external-dtd", false) }
        }.newDocumentBuilder().parse(xml.inputStream())

        private fun resolve(base: String, href: String): String =
            normalize(if (base.isEmpty()) href else "$base/$href")

        /** A path without fragment, percent-encoding, leading slash or `..` steps. */
        fun normalize(href: String): String {
            val path = decode(href.substringBefore('#').substringBefore('?'))
            val parts = ArrayList<String>()
            for (part in path.split('/')) {
                when (part) {
                    "", "." -> {}
                    ".." -> if (parts.isNotEmpty()) parts.removeAt(parts.lastIndex)
                    else -> parts.add(part)
                }
            }
            return parts.joinToString("/")
        }

        private fun decode(s: String): String {
            if ('%' !in s) return s
            val bytes = java.io.ByteArrayOutputStream()
            var i = 0
            while (i < s.length) {
                val c = s[i]
                val hex = if (c == '%' && i + 2 <= s.lastIndex) s.substring(i + 1, i + 3).toIntOrNull(16) else null
                if (hex != null) {
                    bytes.write(hex)
                    i += 3
                } else {
                    bytes.write(c.toString().toByteArray(Charsets.UTF_8))
                    i++
                }
            }
            return bytes.toString("UTF-8")
        }
    }
}

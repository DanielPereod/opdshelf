package io.github.danielpereod.opdshelf.reader

import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import org.junit.Assert.assertEquals
import org.junit.Test

class SpineTest {
    private val container = """<?xml version="1.0"?>
        <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
          <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
        </container>"""

    // Like many EPUBs, the cover is left out of the reading order
    private val opf = """<?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE package>
        <package xmlns="http://www.idpf.org/2007/opf" version="2.0">
          <manifest>
            <item id="cubierta" href="text/cubierta.xhtml" media-type="application/xhtml+xml"/>
            <item id="portada" href="text/portada.xhtml" media-type="application/xhtml+xml"/>
            <item id="c01" href="text/cap%C3%ADtulo%201.xhtml" media-type="application/xhtml+xml"/>
            <item id="notas" href="text/notas.xhtml" media-type="application/xhtml+xml"/>
            <item id="c02" href="text/../text/c02.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="cubierta" linear="no"/>
            <itemref idref="portada"/>
            <itemref idref="c01"/>
            <itemref idref="notas" linear="no"/>
            <itemref idref="c02"/>
          </spine>
        </package>"""

    private fun epub(): File {
        val file = File.createTempFile("book", ".epub")
        file.deleteOnExit()
        ZipOutputStream(file.outputStream()).use { zip ->
            for ((name, text) in listOf("META-INF/container.xml" to container, "OEBPS/content.opf" to opf)) {
                zip.putNextEntry(ZipEntry(name))
                zip.write(text.toByteArray())
                zip.closeEntry()
            }
        }
        return file
    }

    private val readingOrder = listOf("OEBPS/text/portada.xhtml", "OEBPS/text/cap%C3%ADtulo%201.xhtml", "OEBPS/text/c02.xhtml")

    @Test
    fun numbersChaptersLikeKOReaderWhenItemsAreNotLinear() {
        val spine = Spine.of(epub(), readingOrder)
        // The chapter Readium shows second is DocFragment[3]
        assertEquals(listOf(1, 2, 4), readingOrder.indices.map { spine.spineIndex(it) })
        assertEquals(1, spine.readingOrderIndex(spineIndex("/body/DocFragment[3]/body/div/p[40]/text().0")))
        assertEquals(2, spine.readingOrderIndex(4))
        // A non-linear item leads to the next chapter Readium has
        assertEquals(0, spine.readingOrderIndex(0))
        assertEquals(2, spine.readingOrderIndex(3))
        assertEquals(-1, spine.readingOrderIndex(5))
        assertEquals(-1, spine.readingOrderIndex(-1))
    }

    @Test
    fun keepsIndexesWhenTheSpineCannotBeRead() {
        val broken = File.createTempFile("broken", ".epub").apply { deleteOnExit(); writeText("not a zip") }
        val spine = Spine.of(broken, readingOrder)
        assertEquals(2, spine.spineIndex(2))
        assertEquals(2, spine.readingOrderIndex(2))
        // A reading order item missing from the spine
        val other = Spine.of(epub(), readingOrder + "OEBPS/text/extra.xhtml")
        assertEquals(1, other.spineIndex(1))
    }

    @Test
    fun normalizesHrefs() {
        assertEquals("OEBPS/text/capítulo 1.xhtml", Spine.normalize("/OEBPS/./text/../text/cap%C3%ADtulo%201.xhtml#p2"))
    }
}

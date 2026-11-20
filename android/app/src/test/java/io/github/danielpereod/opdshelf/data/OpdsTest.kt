package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class OpdsTest {
    private fun fixture(name: String) = javaClass.classLoader!!.getResourceAsStream(name)!!

    @Test
    fun parsesTheServersBookFeed() {
        val feed = parseFeed(fixture("books.xml"), "http://127.0.0.1:3998/opds/books")
        assertEquals("All books", feed.title)
        assertEquals("http://127.0.0.1:3998/opds/search?q={searchTerms}", feed.searchUrl)
        val book = feed.entries.single()
        assertEquals("Test Book", book.title)
        assertEquals(listOf("Ana Autora"), book.authors)
        assertEquals("http://127.0.0.1:3998/download/Test.epub", book.acquisition!!.url)
        assertEquals("application/epub+zip", book.acquisition!!.type)
        assertEquals(60150L, book.acquisition!!.length)
        assertNull(book.feedUrl)
    }

    @Test
    fun parsesNavigationEntries() {
        val feed = parseFeed(fixture("root.xml"), "http://127.0.0.1:3998/")
        val books = feed.entries.first { it.feedUrl == "http://127.0.0.1:3998/opds/books" }
        assertNull(books.acquisition)
    }

    @Test
    fun resolvesRelativeLinks() {
        val xml = """
            <feed xmlns="http://www.w3.org/2005/Atom"><title>T</title>
            <link rel="next" href="?page=2"/>
            <entry><id>1</id><title>A</title>
              <link rel="http://opds-spec.org/acquisition" href="../download/My Book.epub" type="application/epub+zip"/>
              <link rel="http://opds-spec.org/image/thumbnail" href="/thumb/My%20Book.epub"/>
            </entry></feed>
        """.trimIndent()
        val feed = parseFeed(xml.byteInputStream(), "https://books.example/opds/books")
        assertEquals("https://books.example/opds/books?page=2", feed.nextUrl)
        val entry = feed.entries.single()
        assertEquals("https://books.example/download/My%20Book.epub", entry.acquisition!!.url)
        assertEquals("https://books.example/thumb/My%20Book.epub", entry.thumbnailUrl)
    }

    @Test
    fun knowsWhereABookIsInTheLibrary() {
        val entry = Entry(
            id = "urn:opdshelf:book:Novela%20negra/El%20a%C3%B1o.epub", title = "El año", authors = emptyList(), summary = null,
            feedUrl = null,
            acquisition = Acquisition("http://h:3000/download/Novela%20negra/El%20a%C3%B1o.epub", "application/epub+zip", null, null),
            thumbnailUrl = "http://h:3000/thumb/Novela%20negra/El%20a%C3%B1o.epub?v=1",
            coverUrl = "http://h:3000/cover/Novela%20negra/El%20a%C3%B1o.epub?v=1",
        )
        assertEquals("Novela negra/El año.epub", entry.serverPath)
        assertEquals("Novela%20negra/El%20a%C3%B1o.epub", encodeLibraryPath("Novela negra/El año.epub"))
        assertEquals("a-b_c.~d", encodeLibraryPath("a-b_c.~d"))

        val moved = entry.movedTo("Novela negra/El año.epub", "Sport/El año.epub")
        assertEquals("urn:opdshelf:book:Sport/El%20a%C3%B1o.epub", moved.id)
        assertEquals("http://h:3000/download/Sport/El%20a%C3%B1o.epub", moved.acquisition!!.url)
        assertEquals("http://h:3000/cover/Sport/El%20a%C3%B1o.epub?v=1", moved.coverUrl)
        assertEquals("Sport/El año.epub", moved.serverPath)
        assertNull(entry.copy(acquisition = null).serverPath)
    }

    @Test
    fun folderNamesLoseTheServersEmoji() {
        val xml = """
            <feed xmlns="http://www.w3.org/2005/Atom"><title>Folders</title>
            <entry><id>urn:opdshelf:folder:Comics%20y%20m%C3%A1s</id><title>📁 Comics y más</title><content type="text">0 books</content>
              <link rel="subsection" href="/opds/folders/Comics%20y%20m%C3%A1s" type="application/atom+xml;profile=opds-catalog;kind=acquisition"/></entry>
            <entry><id>urn:opdshelf:folder:Novela/Negra</id><title>📁 Negra</title><content type="text">3 libros</content>
              <link rel="subsection" href="/opds/folders/Novela/Negra" type="application/atom+xml;profile=opds-catalog;kind=acquisition"/></entry>
            <entry><id>urn:opdshelf:author:x</id><title>Ana</title>
              <link rel="subsection" href="/opds/authors/Ana" type="application/atom+xml;profile=opds-catalog;kind=acquisition"/></entry>
            <entry><id>urn:opdshelf:book:x.epub</id><title>📁 A book that starts so</title>
              <link rel="http://opds-spec.org/acquisition" href="/download/x.epub" type="application/epub+zip"/></entry>
            </feed>
        """.trimIndent()
        val (comics, negra, author, book) = parseFeed(xml.byteInputStream(), "http://h:3000/opds/folders").entries
        assertEquals("Comics y más", comics.title)
        assertEquals("Comics y más", comics.folderPath)
        assertEquals(0, comics.folderBookCount)
        assertEquals("Negra", negra.title)
        assertEquals("Novela/Negra", negra.folderPath)
        assertEquals(3, negra.folderBookCount)
        assertNull("authors are not folders", author.folderPath)
        assertEquals("a book keeps its title as it is", "📁 A book that starts so", book.title)
        assertNull(book.folderPath)
        assertEquals("Plain", folderTitle("Plain"))
    }

    @Test
    fun booksFollowTheirFolderWhenItMoves() {
        val id = "urn:opdshelf:book:Novela%20negra/Sub/El%20a%C3%B1o.epub"
        assertEquals("urn:opdshelf:book:Cl%C3%A1sicos/Negra/Sub/El%20a%C3%B1o.epub", bookIdAfterFolderMove(id, "Novela negra", "Clásicos/Negra"))
        assertNull(bookIdAfterFolderMove(id, "Novela", "Otra"))
        assertNull(bookIdAfterFolderMove("urn:opdshelf:book:Novela%20negra.epub", "Novela negra", "X"))
    }
}

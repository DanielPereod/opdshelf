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
}

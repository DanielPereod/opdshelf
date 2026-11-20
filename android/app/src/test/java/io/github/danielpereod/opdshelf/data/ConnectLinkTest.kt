package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ConnectLinkTest {
    @Test
    fun readsThePanelsQrCode() {
        // As the server writes it (src/apps.rs, connect_link)
        val link = ConnectLink.parse("opdshelf://connect?server=http%3A%2F%2Fhost%3A3000&username=ana&password=p%26ss+word")
        assertEquals(ConnectLink("http://host:3000", "ana", "p&ss word"), link)
    }

    @Test
    fun aServerWithoutLogin() {
        assertEquals(ConnectLink("http://h", "", ""), ConnectLink.parse("opdshelf://connect?server=http%3A%2F%2Fh"))
    }

    @Test
    fun otherCodesAreNotLinks() {
        assertNull(ConnectLink.parse("https://example.com/?server=x"))
        assertNull(ConnectLink.parse("opdshelf://connect?username=ana"))
        assertNull(ConnectLink.parse("just some text"))
        assertNull(ConnectLink.parse(null))
    }
}

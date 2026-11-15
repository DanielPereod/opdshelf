package io.github.danielpereod.opdshelf.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AddressTest {
    @Test
    fun addressesAreCheckedBeforeConnecting() {
        assertNull(addressProblem(normalizeServerUrl("192.168.1.10:3000")))
        assertNull(addressProblem(normalizeServerUrl("books.example.com")))
        assertNull(addressProblem("https://books.example/shelf?x=1"))
        assertNull(addressProblem("http://[fe80::1]:8080"))
        assertEquals(AddressProblem.Port, addressProblem(normalizeServerUrl("192.168.1.10:30000000")))
        assertEquals(AddressProblem.Port, addressProblem(normalizeServerUrl("192.168.1.10:abc")))
        assertEquals(AddressProblem.Port, addressProblem(normalizeServerUrl("192.168.1.10:")))
        assertEquals(AddressProblem.Invalid, addressProblem(normalizeServerUrl("my server")))
        assertEquals(AddressProblem.Invalid, addressProblem(normalizeServerUrl("books..example")))
        assertEquals(AddressProblem.Invalid, addressProblem("https://"))
    }
}

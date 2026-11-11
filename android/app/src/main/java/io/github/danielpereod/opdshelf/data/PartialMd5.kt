package io.github.danielpereod.opdshelf.data

import java.io.File
import java.io.RandomAccessFile
import java.security.MessageDigest

/**
 * KOReader's document id, which OPDShelf syncs positions under: the MD5 of
 * 1 KiB samples taken at 0, 1 KiB, 4 KiB, 16 KiB… up to 1 GiB into the file.
 */
fun partialMd5(file: File): String {
    val md5 = MessageDigest.getInstance("MD5")
    RandomAccessFile(file, "r").use { raf ->
        val length = raf.length()
        val buffer = ByteArray(1024)
        for (i in -1..10) {
            val offset = if (i < 0) 0L else 1024L shl (2 * i)
            if (offset >= length) break
            raf.seek(offset)
            var read = 0
            while (read < buffer.size) {
                val n = raf.read(buffer, read, buffer.size - read)
                if (n <= 0) break
                read += n
            }
            md5.update(buffer, 0, read)
        }
    }
    return md5.digest().joinToString("") { "%02x".format(it) }
}

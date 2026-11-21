package io.github.danielpereod.opdshelf.ui

import android.content.Context
import io.github.danielpereod.opdshelf.R
import io.github.danielpereod.opdshelf.data.HttpError
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import javax.net.ssl.SSLException

/** A network error in words that say what to check. */
fun Context.describe(e: Throwable): String = when (e) {
    is SSLException -> getString(R.string.error_tls)
    is UnknownHostException -> getString(R.string.error_host)
    is ConnectException -> getString(R.string.error_refused)
    is SocketTimeoutException -> getString(R.string.error_timeout)
    is HttpError -> when {
        e.code == 401 -> getString(R.string.error_login)
        // What the server refused, by its code, in the app's words
        e.reason == "not_empty" -> getString(R.string.error_folder_not_empty)
        e.reason == "exists" -> getString(R.string.error_exists)
        e.reason == "invalid_name" -> getString(R.string.error_invalid_name)
        e.reason == "into_itself" -> getString(R.string.error_into_itself)
        e.reason == "not_found" -> getString(R.string.error_not_found)
        // The status line of a response ("503 Service Unavailable") is the server's English: say it in the app's language
        e.message.isNullOrBlank() || e.message!!.startsWith("${e.code} ") -> getString(R.string.error_server, e.code)
        else -> e.message ?: e.code.toString()
    }
    // OkHttp refuses an address it cannot read with an English message
    is IllegalArgumentException -> getString(R.string.error_address)
    is java.net.MalformedURLException -> getString(R.string.error_address)
    else -> e.message ?: e.toString()
}

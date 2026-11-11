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
    is HttpError -> if (e.code == 401) getString(R.string.error_login) else e.message ?: e.code.toString()
    else -> e.message ?: e.toString()
}

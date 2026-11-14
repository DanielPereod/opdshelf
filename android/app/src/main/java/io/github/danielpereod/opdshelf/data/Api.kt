package io.github.danielpereod.opdshelf.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.Json
import okhttp3.Credentials
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.File
import java.io.IOException
import java.util.concurrent.TimeUnit

class HttpError(val code: Int, message: String) : IOException(message)

val json = Json { ignoreUnknownKeys = true; encodeDefaults = false; explicitNulls = false }

/** Talks to the OPDShelf server: catalog, downloads and reading positions. */
class Api(private val settings: Settings) {

    /** Sends the login only to the configured server, never to other hosts. */
    private val auth = Interceptor { chain ->
        val request = chain.request()
        val server = settings.serverUrl.toHttpUrlOrNull()
        val sameServer = server != null && request.url.host == server.host && request.url.port == server.port
        if (sameServer && settings.username.isNotEmpty()) {
            chain.proceed(
                request.newBuilder()
                    .header("Authorization", Credentials.basic(settings.username, settings.password, Charsets.UTF_8))
                    .build()
            )
        } else {
            chain.proceed(request)
        }
    }

    val client: OkHttpClient = OkHttpClient.Builder()
        .addInterceptor(auth)
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .build()

    fun url(path: String): String = settings.serverUrl + path

    suspend fun feed(url: String): Feed = withContext(Dispatchers.IO) {
        client.newCall(Request.Builder().url(url).header("Accept", "application/atom+xml").build()).execute().use {
            if (!it.isSuccessful) throw HttpError(it.code, "${it.code} ${it.message}")
            parseFeed(it.body!!.byteStream(), it.request.url.toString())
        }
    }

    /** Downloads to [target], through a temporary file so a cut download never looks complete. */
    suspend fun download(url: String, target: File, onProgress: (Float) -> Unit) = withContext(Dispatchers.IO) {
        val tmp = File(target.parentFile, target.name + ".part")
        client.newCall(Request.Builder().url(url).build()).execute().use { response ->
            if (!response.isSuccessful) throw HttpError(response.code, "${response.code} ${response.message}")
            val body = response.body!!
            val total = body.contentLength()
            tmp.outputStream().use { out ->
                body.byteStream().use { input ->
                    val buffer = ByteArray(64 * 1024)
                    var done = 0L
                    while (true) {
                        val n = input.read(buffer)
                        if (n < 0) break
                        out.write(buffer, 0, n)
                        done += n
                        if (total > 0) onProgress(done.toFloat() / total)
                    }
                }
            }
        }
        if (!tmp.renameTo(target)) throw IOException("Cannot save ${target.name}")
    }

    suspend fun getProgress(document: String): Position? = withContext(Dispatchers.IO) {
        client.newCall(Request.Builder().url(url("/api/progress/$document")).build()).execute().use {
            if (!it.isSuccessful) throw HttpError(it.code, "${it.code} ${it.message}")
            json.decodeFromString(ProgressResponse.serializer(), it.body!!.string()).current
        }
    }

    suspend fun putProgress(document: String, position: Position) = withContext(Dispatchers.IO) {
        // sent_at lets the server correct the timestamp when this device's clock is off
        val fields = json.encodeToJsonElement(Position.serializer(), position).jsonObject +
            ("sent_at" to JsonPrimitive(System.currentTimeMillis() / 1000))
        val body = JsonObject(fields).toString().toRequestBody("application/json".toMediaType())
        client.newCall(Request.Builder().url(url("/api/progress/$document")).put(body).build()).execute().use {
            if (!it.isSuccessful) throw HttpError(it.code, "${it.code} ${it.message}")
        }
    }

    suspend fun getAnnotations(document: String): List<Annotation> = withContext(Dispatchers.IO) {
        client.newCall(Request.Builder().url(url("/api/annotations/$document")).build()).execute().use {
            if (!it.isSuccessful) throw HttpError(it.code, "${it.code} ${it.message}")
            json.decodeFromString(AnnotationsResponse.serializer(), it.body!!.string()).annotations
        }
    }

    /** Sends highlights changed here; answers every highlight of the book. */
    suspend fun putAnnotations(document: String, list: List<Annotation>): List<Annotation> = withContext(Dispatchers.IO) {
        val body = JsonObject(
            mapOf(
                "device" to JsonPrimitive(settings.deviceName),
                "device_id" to JsonPrimitive(settings.deviceId),
                "sent_at" to JsonPrimitive(System.currentTimeMillis() / 1000),
                "annotations" to json.encodeToJsonElement(kotlinx.serialization.builtins.ListSerializer(Annotation.serializer()), list),
            )
        ).toString().toRequestBody("application/json".toMediaType())
        client.newCall(Request.Builder().url(url("/api/annotations/$document")).put(body).build()).execute().use {
            if (!it.isSuccessful) throw HttpError(it.code, "${it.code} ${it.message}")
            json.decodeFromString(AnnotationsResponse.serializer(), it.body!!.string()).annotations
        }
    }

    /** Every folder of the library, sorted by path. */
    suspend fun folders(): List<ServerFolder> = withContext(Dispatchers.IO) {
        client.newCall(Request.Builder().url(url("/api/folders")).build()).execute().use {
            if (!it.isSuccessful) throw HttpError(it.code, "${it.code} ${it.message}")
            json.decodeFromString(FoldersResponse.serializer(), it.body!!.string()).folders
        }
    }

    /** Creates [name] inside [parent] ("" for the top) and answers its path. */
    suspend fun createFolder(parent: String, name: String): String = withContext(Dispatchers.IO) {
        val body = JsonObject(mapOf("parent" to JsonPrimitive(parent), "name" to JsonPrimitive(name)))
            .toString().toRequestBody("application/json".toMediaType())
        client.newCall(Request.Builder().url(url("/api/folders")).post(body).build()).execute().use {
            val text = it.body?.string().orEmpty()
            if (!it.isSuccessful) throw HttpError(it.code, serverError(text) ?: "${it.code} ${it.message}")
            json.decodeFromString(CreatedFolder.serializer(), text).path
        }
    }

    /** Moves books (by their path in the library) into [folder], "" for the top; it is created if needed. */
    suspend fun moveBooks(paths: List<String>, folder: String): MoveResult = withContext(Dispatchers.IO) {
        val body = JsonObject(
            mapOf(
                "paths" to kotlinx.serialization.json.JsonArray(paths.map { JsonPrimitive(it) }),
                "folder" to JsonPrimitive(folder),
            )
        ).toString().toRequestBody("application/json".toMediaType())
        client.newCall(Request.Builder().url(url("/api/books/move")).post(body).build()).execute().use {
            val text = it.body?.string().orEmpty()
            val result = runCatching { json.decodeFromString(MoveResult.serializer(), text) }.getOrNull()
            if (!it.isSuccessful || result == null) {
                throw HttpError(it.code, result?.failed?.firstOrNull() ?: serverError(text) ?: "${it.code} ${it.message}")
            }
            result
        }
    }

    private fun serverError(text: String): String? =
        runCatching { json.decodeFromString(ErrorResponse.serializer(), text).error }.getOrNull()

    /**
     * Uploads a book to [folder] on the server ("" for the top), reading it from [open].
     * The server never overwrites: it answers the path it saved the book under.
     */
    suspend fun upload(
        folder: String,
        name: String,
        size: Long,
        open: () -> java.io.InputStream,
        onProgress: (Float) -> Unit,
    ): String = withContext(Dispatchers.IO) {
        val path = (if (folder.isEmpty()) name else "$folder/$name").split('/').joinToString("/") { segment ->
            java.net.URLEncoder.encode(segment, "UTF-8").replace("+", "%20")
        }
        val body = object : okhttp3.RequestBody() {
            override fun contentType() = "application/octet-stream".toMediaType()
            override fun contentLength() = size
            override fun writeTo(sink: okio.BufferedSink) {
                open().use { input ->
                    val buffer = ByteArray(64 * 1024)
                    var done = 0L
                    while (true) {
                        val n = input.read(buffer)
                        if (n < 0) break
                        sink.write(buffer, 0, n)
                        done += n
                        if (size > 0) onProgress(done.toFloat() / size)
                    }
                }
            }
        }
        // Big books on slow connections take a while to go up
        val slow = client.newBuilder().writeTimeout(5, TimeUnit.MINUTES).readTimeout(2, TimeUnit.MINUTES).build()
        slow.newCall(Request.Builder().url(url("/api/upload/$path")).put(body).build()).execute().use {
            val text = it.body?.string().orEmpty()
            val answer = runCatching { json.parseToJsonElement(text).jsonObject }.getOrNull()
            if (!it.isSuccessful) {
                val reason = (answer?.get("error") as? JsonPrimitive)?.content ?: "${it.code} ${it.message}"
                throw HttpError(it.code, reason)
            }
            (answer?.get("path") as? JsonPrimitive)?.content ?: name
        }
    }

    /** Checks the address and login. */
    suspend fun check() {
        feed(url("/"))
    }
}

/** A folder of the library on the server. */
@kotlinx.serialization.Serializable
data class ServerFolder(val path: String, val name: String = path.substringAfterLast('/'), val count: Int = 0) {
    val depth get() = path.count { it == '/' }
}

@kotlinx.serialization.Serializable
private data class FoldersResponse(val folders: List<ServerFolder> = emptyList())

@kotlinx.serialization.Serializable
private data class CreatedFolder(val path: String)

@kotlinx.serialization.Serializable
private data class ErrorResponse(val error: String? = null)

@kotlinx.serialization.Serializable
data class MoveResult(val moved: Int = 0, val folder: String = "", val failed: List<String> = emptyList())

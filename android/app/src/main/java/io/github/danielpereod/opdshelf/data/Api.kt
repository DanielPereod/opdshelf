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

    /** Checks the address and login. */
    suspend fun check() {
        feed(url("/"))
    }
}

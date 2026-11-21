package io.github.danielpereod.opdshelf.sync

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import io.github.danielpereod.opdshelf.app
import io.github.danielpereod.opdshelf.data.Annotation
import io.github.danielpereod.opdshelf.data.Api
import io.github.danielpereod.opdshelf.data.HttpError
import io.github.danielpereod.opdshelf.data.LibraryProgress
import io.github.danielpereod.opdshelf.data.clearedByDocument
import io.github.danielpereod.opdshelf.data.marksByDocument
import io.github.danielpereod.opdshelf.data.LocalPosition
import io.github.danielpereod.opdshelf.data.LocalStore
import io.github.danielpereod.opdshelf.data.Position
import io.github.danielpereod.opdshelf.data.Settings
import io.github.danielpereod.opdshelf.data.StartPosition
import io.github.danielpereod.opdshelf.data.chooseStart
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.TimeUnit

/**
 * Reading positions: saved on the device first, then sent to the server by a
 * background job that waits for a connection, so reading offline just works.
 */
/** What the user sees of syncing: positions not sent yet and when it last worked. */
data class SyncStatus(
    val pending: Int = 0,
    val lastSync: Long = 0,
    val syncing: Boolean = false,
    /** The last attempt could not reach the server. */
    val failing: Boolean = false,
)

class ReadingSync(
    private val context: Context,
    private val settings: Settings,
    private val api: Api,
    private val store: LocalStore,
) {
    private val _status = MutableStateFlow(SyncStatus(pending = pendingCount(), lastSync = settings.lastSync))

    private fun pendingCount() =
        store.pending().size + store.pendingAnnotations().values.sumOf { it.size } + store.pendingMarks().size
    val status: StateFlow<SyncStatus> = _status.asStateFlow()

    private val _library = MutableStateFlow<LibraryProgress?>(null)
    /** Every book's position and finished mark on the server, as last fetched; null until then. */
    val library: StateFlow<LibraryProgress?> = _library.asStateFlow()

    /**
     * Fetches every book's position and mark, and applies the marks to the books
     * on this device, so a book finished on the web or KOReader is finished here too.
     */
    suspend fun refreshLibrary(): Boolean = runCatching {
        val progress = api.libraryProgress()
        store.adoptMarks(progress.marksByDocument())
        store.adoptCleared(progress.clearedByDocument())
        store.adoptPositions(progress.books.mapNotNull { b -> b.current?.let { b.document to it } }.toMap())
        _library.value = progress
        true
    }.getOrDefault(false)

    /** Marks a book as finished or not, here at once and on the server when possible. */
    fun setFinished(id: String, finished: Boolean) {
        store.setFinished(id, finished)
        _status.update { it.copy(pending = pendingCount()) }
        schedule(0)
    }

    /** Where to open a book: the newest of this device's position and the server's. */
    suspend fun start(document: String): StartPosition {
        val local = store.position(document)
        val answer = runCatching { api.bookProgress(document) }.getOrElse {
            return StartPosition(local?.position, local?.locator, null)
        }
        answer.mark?.let { store.adoptMarks(mapOf(document to it)) }
        val server = answer.current
        if (local?.pending == true) schedule()
        val start = chooseStart(local, server, settings.deviceId)
        // A position of this device not sent yet is kept and sent: the server decides which one is
        // newer (it corrects this device's clock), instead of it being dropped here
        if (start.position != null && start.position == server && local?.pending != true) {
            store.savePosition(document, LocalPosition(server, pending = false, locator = start.locator))
            store.setPercentage(document, server.percentage, server.timestamp)
        }
        return start
    }

    /** Saves a position and sends it when possible. */
    fun save(document: String, position: Position, locator: String?, now: Boolean = false) {
        val stamped = position.copy(
            device = settings.deviceName,
            deviceId = settings.deviceId,
            timestamp = System.currentTimeMillis() / 1000,
        )
        store.savePosition(document, LocalPosition(stamped, pending = true, locator = locator))
        store.setPercentage(document, stamped.percentage)
        _status.update { it.copy(pending = pendingCount()) }
        schedule(if (now) 0 else 2)
    }

    /** Saves a new or changed highlight (or a deletion) and sends it when possible. */
    fun annotate(document: String, annotation: Annotation): Annotation {
        val now = System.currentTimeMillis() / 1000
        val before = store.annotations(document).find(annotation.id)
        val stamped = annotation.copy(
            updated = maxOf(now, before?.updated ?: 0),
            created = annotation.created.takeIf { it > 0 } ?: before?.created ?: now,
            device = settings.deviceName,
            deviceId = settings.deviceId,
        )
        store.updateAnnotations(document) { it.copy(pending = it.pending + (stamped.id to stamped)) }
        _status.update { it.copy(pending = pendingCount()) }
        schedule(1)
        return stamped
    }

    /** Gets a book's highlights from the server, keeping changes not sent yet. */
    suspend fun refreshAnnotations(document: String): Boolean = runCatching {
        val server = api.getAnnotations(document)
        store.updateAnnotations(document) { it.adopt(server) }
        if (store.annotations(document).pending.isNotEmpty()) schedule(0)
        true
    }.getOrDefault(false)

    /** Sends every pending position now. Returns false if some could not be sent. */
    suspend fun flush(): Boolean = flushLock.withLock {
        try {
            sendPending()
        } finally {
            // Cancelled midway (the job was stopped): not still "syncing"
            _status.update { it.copy(syncing = false) }
        }
    }

    /** One sending at a time: "Sync" while the background job is working must not send everything twice. */
    private val flushLock = kotlinx.coroutines.sync.Mutex()

    private suspend fun sendPending(): Boolean {
        _status.update { it.copy(syncing = true) }
        var ok = true
        for ((document, batch) in store.pendingAnnotations()) {
            try {
                val answer = api.putAnnotations(document, batch)
                store.updateAnnotations(document) { it.sent(batch).adopt(answer) }
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: HttpError) {
                if (e.rejected) {
                    store.updateAnnotations(document) { it.sent(batch) }
                } else {
                    ok = false
                }
            } catch (e: Exception) {
                ok = false
            }
        }
        for ((document, mark) in store.pendingMarks()) {
            try {
                store.markSent(document, mark, api.putMark(document, mark))
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: HttpError) {
                if (e.rejected) {
                    store.markSent(document, mark, null)
                } else {
                    ok = false
                }
            } catch (e: Exception) {
                ok = false
            }
        }
        for ((document, position) in store.pending()) {
            try {
                api.putProgress(document, position)
                store.markSent(document, position)
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: HttpError) {
                // Only OPDShelf's own refusal (a bad request) is final; anything else is tried again
                if (e.rejected) {
                    store.markSent(document, position)
                } else {
                    ok = false
                }
            } catch (e: Exception) {
                ok = false
            }
        }
        // Marks made on other devices come back the same way
        if (ok) refreshLibrary()
        if (ok) settings.lastSync = System.currentTimeMillis()
        _status.value = SyncStatus(pendingCount(), settings.lastSync, syncing = false, failing = !ok)
        return ok
    }

    fun schedule(delaySeconds: Long = 2) {
        if (!settings.isConfigured) return
        val request = OneTimeWorkRequestBuilder<SyncWorker>()
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setInitialDelay(delaySeconds, TimeUnit.SECONDS)
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork("sync-positions", ExistingWorkPolicy.APPEND_OR_REPLACE, request)
    }
}

class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result =
        if (applicationContext.app.sync.flush()) Result.success() else Result.retry()
}

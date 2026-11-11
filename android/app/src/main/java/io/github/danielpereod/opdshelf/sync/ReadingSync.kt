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
import io.github.danielpereod.opdshelf.data.Api
import io.github.danielpereod.opdshelf.data.HttpError
import io.github.danielpereod.opdshelf.data.LocalPosition
import io.github.danielpereod.opdshelf.data.LocalStore
import io.github.danielpereod.opdshelf.data.Position
import io.github.danielpereod.opdshelf.data.Settings
import io.github.danielpereod.opdshelf.data.StartPosition
import io.github.danielpereod.opdshelf.data.chooseStart
import java.util.concurrent.TimeUnit

/**
 * Reading positions: saved on the device first, then sent to the server by a
 * background job that waits for a connection, so reading offline just works.
 */
class ReadingSync(
    private val context: Context,
    private val settings: Settings,
    private val api: Api,
    private val store: LocalStore,
) {
    /** Where to open a book: the newest of this device's position and the server's. */
    suspend fun start(document: String): StartPosition {
        val local = store.position(document)
        val server = runCatching { api.getProgress(document) }.getOrElse {
            return StartPosition(local?.position, local?.locator, null)
        }
        if (local?.pending == true) schedule()
        val start = chooseStart(local, server, settings.deviceId)
        if (start.position != null && start.position == server) {
            store.savePosition(document, LocalPosition(server, pending = false, locator = start.locator))
            store.setPercentage(document, server.percentage)
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
        schedule(if (now) 0 else 2)
    }

    /** Sends every pending position now. Returns false if some could not be sent. */
    suspend fun flush(): Boolean {
        var ok = true
        for ((document, position) in store.pending()) {
            try {
                api.putProgress(document, position)
                store.markSent(document, position)
            } catch (e: HttpError) {
                // The server will never take it (e.g. a bad request): stop retrying
                if (e.code in 400..499 && e.code != 401 && e.code != 408 && e.code != 429) {
                    store.markSent(document, position)
                } else {
                    ok = false
                }
            } catch (e: Exception) {
                ok = false
            }
        }
        return ok
    }

    fun schedule(delaySeconds: Long = 2) {
        if (!settings.isConfigured) return
        val request = OneTimeWorkRequestBuilder<SyncWorker>()
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setInitialDelay(delaySeconds, TimeUnit.SECONDS)
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork("sync-positions", ExistingWorkPolicy.REPLACE, request)
    }
}

class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result =
        if (applicationContext.app.sync.flush()) Result.success() else Result.retry()
}

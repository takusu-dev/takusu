package expo.modules.takusuwidget

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

/**
 * Periodic worker that refreshes the widget.
 *
 * It tries to fetch from the local Rust server (127.0.0.1:3838). If the
 * server is not running, it starts it via the UniFFI bindings, fetches,
 * then stops it. The fetched snapshot is persisted to SharedPreferences
 * and the widget is updated.
 */
class WidgetUpdateWorker(
    context: Context,
    params: WorkerParameters,
) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val prefs = applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val token = prefs.getString(KEY_TOKEN, null) ?: return Result.success()
        val workersUrl = prefs.getString(KEY_WORKERS_URL, null) ?: return Result.success()

        // Try fetching from the local server first (it may already be running
        // if the app is in the foreground).
        var snapshot = WidgetFetcher.fetch(token)

        if (snapshot == null) {
            // Local server not running — start it via UniFFI, fetch, then stop.
            snapshot = startServerAndFetch(workersUrl, token)
        }

        if (snapshot != null) {
            persistSnapshot(prefs, snapshot)
            TakusuWidgetProvider.updateWidget(applicationContext)
            return Result.success()
        }

        // Fetch failed entirely — keep the last snapshot, just retry later.
        return Result.retry()
    }

    private fun startServerAndFetch(
        workersUrl: String,
        token: String,
    ): WidgetSnapshot? =
        try {
            val server = uniffi.takusu_android.TakusuServer()
            try {
                server.start(3838.toUShort(), workersUrl, token)
                // Give the server a moment to bind
                Thread.sleep(500)
                WidgetFetcher.fetch(token)
            } finally {
                try {
                    server.stop()
                } catch (_: Exception) {
                }
                try {
                    server.destroy()
                } catch (_: Exception) {
                }
            }
        } catch (e: Exception) {
            null
        }

    companion object {
        const val PREFS_NAME = "takusu_widget"
        const val KEY_TOKEN = "token"
        const val KEY_WORKERS_URL = "workers_url"
        const val KEY_SNAPSHOT = "snapshot_json"
        const val KEY_UPDATED_AT = "updated_at"
        const val WORK_NAME = "takusu_widget_update"

        fun persistSnapshot(
            prefs: android.content.SharedPreferences,
            s: WidgetSnapshot,
        ) {
            val arr = org.json.JSONArray()
            for (u in s.upcoming) {
                val o = org.json.JSONObject()
                o.put("id", u.id)
                o.put("title", u.title)
                o.put("start_at", u.startAt ?: org.json.JSONObject.NULL)
                o.put("end_at", u.endAt)
                arr.put(o)
            }
            val snap = org.json.JSONObject()
            snap.put("doing_title", s.doingTitle ?: org.json.JSONObject.NULL)
            snap.put("upcoming", arr)
            snap.put("unscheduled_count", s.unscheduledCount)
            prefs
                .edit()
                .putString(KEY_SNAPSHOT, snap.toString())
                .putLong(KEY_UPDATED_AT, System.currentTimeMillis())
                .apply()
        }

        fun schedule(context: Context) {
            val constraints =
                Constraints
                    .Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build()
            val request =
                PeriodicWorkRequestBuilder<WidgetUpdateWorker>(30, TimeUnit.MINUTES)
                    .setConstraints(constraints)
                    .build()
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                WORK_NAME,
                ExistingPeriodicWorkPolicy.KEEP,
                request,
            )
        }

        fun cancel(context: Context) {
            WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
        }
    }
}

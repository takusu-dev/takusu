package expo.modules.takusuwidget

import java.net.HttpURLConnection
import java.net.URL
import org.json.JSONArray
import org.json.JSONObject

/**
 * Fetches the widget snapshot from the local Rust server (127.0.0.1:3838).
 *
 * The local server is started by [WidgetUpdateWorker] before calling this.
 * Returns null on any failure so the caller can fall back to the cached
 * snapshot in SharedPreferences.
 */
data class WidgetSnapshot(
    val doingTitle: String?,
    val upcoming: List<UpcomingTask>,
    val unscheduledCount: Int,
)

data class UpcomingTask(
    val id: String,
    val title: String,
    val startAt: String?,
    val endAt: String,
)

object WidgetFetcher {
    private const val BASE = "http://127.0.0.1:3838"
    private const val CONNECT_TIMEOUT_MS = 3_000
    private const val READ_TIMEOUT_MS = 5_000

    fun fetch(token: String): WidgetSnapshot? {
        return try {
            val tasks = fetchJsonArray("$BASE/api/tasks", token) ?: return null
            val schedule = fetchJsonObject("$BASE/api/schedule", token)
            val scheduleMap = parseScheduleMap(schedule)

            var doing: String? = null
            val upcoming = mutableListOf<UpcomingTask>()
            var unscheduled = 0
            val now = System.currentTimeMillis()

            for (i in 0 until tasks.length()) {
                val t = tasks.getJSONObject(i)
                val status = t.optString("status")
                when (status) {
                    "in_progress" -> {
                        if (doing == null) doing = t.optString("title")
                    }

                    "pending" -> {
                        unscheduled++
                    }

                    "scheduled", "completed", "skipped" -> {
                        val entry = scheduleMap[t.getString("id")]
                        // optString returns literal "null" for JSON null, so
                        // check isNull first to get a real Kotlin null.
                        val startAt = entry?.first ?: optStringOrNull(t, "start_at")
                        val endAt = entry?.second ?: t.getString("end_at")
                        // Skip past completed/skipped tasks from the upcoming list
                        if (status == "completed" || status == "skipped") {
                            // past — skip
                        } else {
                            val endTime = parseIso(endAt) ?: 0L
                            if (endTime >= now) {
                                upcoming.add(UpcomingTask(t.getString("id"), t.getString("title"), startAt, endAt))
                            }
                        }
                    }
                }
            }

            upcoming.sortBy { parseIso(it.startAt ?: it.endAt) ?: Long.MAX_VALUE }

            WidgetSnapshot(
                doingTitle = doing,
                upcoming = upcoming,
                unscheduledCount = unscheduled,
            )
        } catch (e: Exception) {
            null
        }
    }

    private fun fetchJsonArray(
        url: String,
        token: String,
    ): JSONArray? {
        val conn =
            (URL(url).openConnection() as HttpURLConnection).apply {
                connectTimeout = CONNECT_TIMEOUT_MS
                readTimeout = READ_TIMEOUT_MS
                setRequestProperty("Authorization", "Bearer $token")
            }
        return try {
            if (conn.responseCode != 200) return null
            val body = conn.inputStream.bufferedReader().use { it.readText() }
            JSONArray(body)
        } finally {
            conn.disconnect()
        }
    }

    private fun fetchJsonObject(
        url: String,
        token: String,
    ): JSONObject? {
        val conn =
            (URL(url).openConnection() as HttpURLConnection).apply {
                connectTimeout = CONNECT_TIMEOUT_MS
                readTimeout = READ_TIMEOUT_MS
                setRequestProperty("Authorization", "Bearer $token")
            }
        return try {
            if (conn.responseCode != 200) return null
            val body = conn.inputStream.bufferedReader().use { it.readText() }
            JSONObject(body)
        } finally {
            conn.disconnect()
        }
    }

    private fun parseScheduleMap(sched: JSONObject?): Map<String, Pair<String?, String>> {
        if (sched == null) return emptyMap()
        val map = mutableMapOf<String, Pair<String?, String>>()
        // The API returns `schedule` as a JSON-encoded string (not a JSON
        // array), so we need to parse the string value into a JSONArray.
        // If the field is missing or null, return an empty map.
        val scheduleStr = sched.optString("schedule", "")
        if (scheduleStr.isEmpty()) return map
        val entries = JSONArray(scheduleStr)
        for (i in 0 until entries.length()) {
            val e = entries.getJSONObject(i)
            val id = e.getString("task_id")
            val startAt = if (e.isNull("start_at")) null else e.optString("start_at", null)
            val endAt = e.getString("end_at")
            map[id] = Pair(startAt, endAt)
        }
        return map
    }

    // / Returns the string value of [key] from [obj], or null if the key
    // / is absent or explicitly JSON null. Android's `optString(key, null)`
    // / returns the literal string "null" for JSON null values, so this
    // / helper checks `isNull` first.
    private fun optStringOrNull(
        obj: JSONObject,
        key: String,
    ): String? = if (obj.isNull(key)) null else obj.optString(key, null)

    private fun parseIso(iso: String?): Long? {
        if (iso == null) return null
        return try {
            // Trim trailing 'Z' or fractional seconds for compatibility
            val s = iso.replace(Regex("\\.\\d+"), "").replace("Z", "+00:00")
            java.time.OffsetDateTime
                .parse(s)
                .toInstant()
                .toEpochMilli()
        } catch (e: Exception) {
            try {
                java.time.LocalDateTime
                    .parse(iso.replace("Z", ""))
                    .atZone(java.time.ZoneOffset.UTC)
                    .toInstant()
                    .toEpochMilli()
            } catch (e2: Exception) {
                null
            }
        }
    }
}

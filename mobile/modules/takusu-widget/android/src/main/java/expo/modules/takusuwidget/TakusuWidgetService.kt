package expo.modules.takusuwidget

import android.content.Context
import android.content.Intent
import android.widget.RemoteViews
import android.widget.RemoteViewsService
import org.json.JSONArray
import org.json.JSONObject

/**
 * RemoteViewsService that backs the widget's upcoming-tasks ListView.
 *
 * The factory reads the snapshot JSON from SharedPreferences (written by
 * [WidgetUpdateWorker]) and produces RemoteViews rows on demand. This lets
 * the widget scroll through an arbitrary number of upcoming tasks without
 * pre-allocating fixed slots.
 */
class TakusuWidgetService : RemoteViewsService() {
    override fun onGetViewFactory(intent: Intent): RemoteViewsFactory = UpcomingTasksFactory(applicationContext)
}

private class UpcomingTasksFactory(
    private val context: Context,
) : RemoteViewsService.RemoteViewsFactory {
    private var upcoming: JSONArray = JSONArray()

    override fun onCreate() {}

    override fun onDataSetChanged() {
        val prefs =
            context.getSharedPreferences(WidgetUpdateWorker.PREFS_NAME, Context.MODE_PRIVATE)
        val snapshotJson = prefs.getString(WidgetUpdateWorker.KEY_SNAPSHOT, null)
        upcoming =
            if (snapshotJson != null) {
                JSONObject(snapshotJson).optJSONArray("upcoming") ?: JSONArray()
            } else {
                JSONArray()
            }
    }

    override fun onDestroy() {
        upcoming = JSONArray()
    }

    override fun getCount(): Int = upcoming.length()

    override fun getViewAt(position: Int): RemoteViews {
        val views = RemoteViews(context.packageName, R.layout.takusu_widget_item)
        if (position >= upcoming.length()) return views

        val t = upcoming.getJSONObject(position)
        val title = t.getString("title")
        val startAt = if (t.isNull("start_at")) null else t.optString("start_at", null)
        val time = if (startAt != null) formatTime(startAt) else ""
        val text = if (time.isNotEmpty()) "$time  $title" else title
        views.setTextViewText(R.id.widget_item_text, text)

        // Per-item click: fill in the task id into the template intent set
        // by the widget provider. The template has data=takusu://task/ and
        // we append the task id to the URI.
        val taskId = t.optString("id", "")
        val fillIn = Intent(Intent.ACTION_VIEW)
        fillIn.data = android.net.Uri.parse("takusu://task/$taskId")
        views.setOnClickFillInIntent(R.id.widget_item_text, fillIn)

        return views
    }

    override fun getLoadingView(): RemoteViews? = null

    override fun getViewTypeCount(): Int = 1

    override fun getItemId(position: Int): Long = position.toLong()

    override fun hasStableIds(): Boolean = false

    private fun formatTime(iso: String): String =
        try {
            val s = iso.replace(Regex("\\.\\d+"), "").replace("Z", "+00:00")
            val odt = java.time.OffsetDateTime.parse(s)
            val local = odt.atZoneSameInstant(java.time.ZoneId.systemDefault())
            local.format(
                java.time.format.DateTimeFormatter
                    .ofPattern("HH:mm"),
            )
        } catch (e: Exception) {
            try {
                val ldt = java.time.LocalDateTime.parse(iso.replace("Z", ""))
                val local =
                    ldt
                        .atZone(java.time.ZoneOffset.UTC)
                        .withZoneSameInstant(java.time.ZoneId.systemDefault())
                local.format(
                    java.time.format.DateTimeFormatter
                        .ofPattern("HH:mm"),
                )
            } catch (e2: Exception) {
                ""
            }
        }
}

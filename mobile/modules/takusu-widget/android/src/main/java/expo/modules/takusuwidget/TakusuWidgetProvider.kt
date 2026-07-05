package expo.modules.takusuwidget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.widget.RemoteViews
import org.json.JSONArray
import org.json.JSONObject

/**
 * The AppWidgetProvider for the takusu home screen widget.
 *
 * It reads the latest snapshot from SharedPreferences (written by
 * [WidgetUpdateWorker]) and renders it into a RemoteViews tree. If no
 * snapshot is available yet, it shows a placeholder.
 */
class TakusuWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray,
    ) {
        for (id in appWidgetIds) {
            updateWidget(context, appWidgetManager, id)
        }
    }

    override fun onEnabled(context: Context) {
        super.onEnabled(context)
        WidgetUpdateWorker.schedule(context)
    }

    override fun onDisabled(context: Context) {
        super.onDisabled(context)
        // Only cancel periodic work when the last widget instance is removed.
        WidgetUpdateWorker.cancel(context)
    }

    companion object {
        fun updateWidget(context: Context) {
            val manager = AppWidgetManager.getInstance(context)
            val ids = manager.getAppWidgetIds(ComponentName(context, TakusuWidgetProvider::class.java))
            for (id in ids) {
                updateWidget(context, manager, id)
            }
        }

        private fun updateWidget(
            context: Context,
            manager: AppWidgetManager,
            widgetId: Int,
        ) {
            val prefs = context.getSharedPreferences(WidgetUpdateWorker.PREFS_NAME, Context.MODE_PRIVATE)
            val snapshotJson = prefs.getString(WidgetUpdateWorker.KEY_SNAPSHOT, null)
            val updatedAt = prefs.getLong(WidgetUpdateWorker.KEY_UPDATED_AT, 0L)

            val views = RemoteViews(context.packageName, R.layout.takusu_widget)

            if (snapshotJson != null) {
                val snap = JSONObject(snapshotJson)
                renderSnapshot(context, views, snap, updatedAt)
            } else {
                renderPlaceholder(views)
            }

            // Tap on widget → open the app
            val launchIntent = context.packageManager.getLaunchIntentForPackage(context.packageName)
            if (launchIntent != null) {
                val pi =
                    PendingIntent.getActivity(
                        context,
                        0,
                        launchIntent,
                        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                    )
                views.setOnClickPendingIntent(R.id.widget_root, pi)
            }

            manager.updateAppWidget(widgetId, views)
        }

        private fun renderSnapshot(
            context: Context,
            views: RemoteViews,
            snap: JSONObject,
            updatedAt: Long,
        ) {
            // Doing task
            val doingTitle = if (snap.isNull("doing_title")) null else snap.optString("doing_title", null)
            if (doingTitle != null) {
                views.setTextViewText(R.id.widget_doing_label, "進行中")
                views.setTextViewText(R.id.widget_doing_title, doingTitle)
                views.setViewVisibility(R.id.widget_doing_section, android.view.View.VISIBLE)
            } else {
                views.setViewVisibility(R.id.widget_doing_section, android.view.View.GONE)
            }

            // Upcoming tasks
            val upcoming = snap.optJSONArray("upcoming") ?: JSONArray()
            views.setTextViewText(R.id.widget_upcoming_label, "次のタスク (${upcoming.length()})")

            // Show up to 3 upcoming tasks in the pre-defined slots
            val slotIds = intArrayOf(R.id.widget_upcoming_1, R.id.widget_upcoming_2, R.id.widget_upcoming_3)
            for (i in slotIds.indices) {
                if (i < upcoming.length()) {
                    val t = upcoming.getJSONObject(i)
                    val title = t.getString("title")
                    val startAt = if (t.isNull("start_at")) null else t.optString("start_at", null)
                    val time = if (startAt != null) formatTime(startAt) else ""
                    val text = if (time.isNotEmpty()) "$time  $title" else title
                    views.setTextViewText(slotIds[i], text)
                    views.setViewVisibility(slotIds[i], android.view.View.VISIBLE)
                } else {
                    views.setViewVisibility(slotIds[i], android.view.View.GONE)
                }
            }

            // Unscheduled count
            val unscheduled = snap.optInt("unscheduled_count", 0)
            views.setTextViewText(R.id.widget_unscheduled, "未スケジュール: $unscheduled")

            // Last updated time
            if (updatedAt > 0L) {
                val fmt = java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault())
                views.setTextViewText(R.id.widget_updated, "更新: ${fmt.format(java.util.Date(updatedAt))}")
            }
        }

        private fun renderPlaceholder(views: RemoteViews) {
            views.setViewVisibility(R.id.widget_doing_section, android.view.View.GONE)
            views.setTextViewText(R.id.widget_upcoming_label, "次のタスク")
            views.setTextViewText(R.id.widget_upcoming_1, "アプリを起動して設定してください")
            views.setViewVisibility(R.id.widget_upcoming_1, android.view.View.VISIBLE)
            views.setViewVisibility(R.id.widget_upcoming_2, android.view.View.GONE)
            views.setViewVisibility(R.id.widget_upcoming_3, android.view.View.GONE)
            views.setTextViewText(R.id.widget_unscheduled, "")
            views.setTextViewText(R.id.widget_updated, "")
        }

        private fun formatTime(iso: String): String =
            try {
                val s = iso.replace(Regex("\\.\\d+"), "").replace("Z", "+00:00")
                val odt = java.time.OffsetDateTime.parse(s)
                val local = odt.atZoneSameInstant(java.time.ZoneId.systemDefault())
                val fmt =
                    java.time.format.DateTimeFormatter
                        .ofPattern("HH:mm")
                local.format(fmt)
            } catch (e: Exception) {
                try {
                    val ldt = java.time.LocalDateTime.parse(iso.replace("Z", ""))
                    val local =
                        ldt
                            .atZone(java.time.ZoneOffset.UTC)
                            .withZoneSameInstant(java.time.ZoneId.systemDefault())
                    val fmt =
                        java.time.format.DateTimeFormatter
                            .ofPattern("HH:mm")
                    local.format(fmt)
                } catch (e2: Exception) {
                    ""
                }
            }
    }
}

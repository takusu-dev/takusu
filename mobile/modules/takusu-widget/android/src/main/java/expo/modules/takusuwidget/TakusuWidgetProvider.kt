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
                renderSnapshot(views, snap, updatedAt)
            } else {
                renderPlaceholder(views)
            }

            // Set up the RemoteAdapter for the ListView — always set, even
            // when showing the placeholder, so the list has a data source on
            // first install before any data is fetched. The factory handles
            // the empty-snapshot case correctly.
            val remoteAdapter =
                android.content.Intent(context, TakusuWidgetService::class.java)
            views.setRemoteAdapter(R.id.widget_upcoming_list, remoteAdapter)

            // Template PendingIntent for per-item clicks. The factory fills
            // in the full data URI (takusu://task/<id>) via
            // setOnClickFillInIntent. The template must NOT set its own data
            // field, otherwise Intent.fillIn() will not override it.
            val templateIntent = Intent(Intent.ACTION_VIEW)
            templateIntent.flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
            val templatePi =
                PendingIntent.getActivity(
                    context,
                    0,
                    templateIntent,
                    PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
            views.setPendingIntentTemplate(R.id.widget_upcoming_list, templatePi)

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

            // Notify the remote adapter to reload its data from SharedPreferences.
            manager.notifyAppWidgetViewDataChanged(
                intArrayOf(widgetId),
                R.id.widget_upcoming_list,
            )
        }

        private fun renderSnapshot(
            views: RemoteViews,
            snap: JSONObject,
            updatedAt: Long,
        ) {
            // Doing tasks
            val doingTitles = snap.optJSONArray("doing_titles")
            if (doingTitles != null && doingTitles.length() > 0) {
                views.setTextViewText(R.id.widget_doing_label, "進行中")
                val titles = mutableListOf<String>()
                for (i in 0 until doingTitles.length()) {
                    titles.add(doingTitles.getString(i))
                }
                views.setTextViewText(R.id.widget_doing_title, titles.joinToString(" / "))
                views.setViewVisibility(R.id.widget_doing_section, android.view.View.VISIBLE)
            } else {
                views.setViewVisibility(R.id.widget_doing_section, android.view.View.GONE)
            }

            // Upcoming tasks
            val upcoming = snap.optJSONArray("upcoming") ?: JSONArray()
            views.setTextViewText(R.id.widget_upcoming_label, "次のタスク (${upcoming.length()})")

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
            // Empty list will show nothing; use the label as a hint.
            views.setTextViewText(R.id.widget_upcoming_label, "アプリを起動して設定してください")
            views.setTextViewText(R.id.widget_unscheduled, "")
            views.setTextViewText(R.id.widget_updated, "")
        }
    }
}

package expo.modules.takusuwidget

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.RemoteViews
import androidx.core.widget.RemoteViewsCompat
import org.json.JSONArray
import org.json.JSONObject

/**
 * The AppWidgetProvider for the takusu home screen widget.
 *
 * It reads the latest snapshot from SharedPreferences (written by
 * [WidgetUpdateWorker]) and renders it into a RemoteViews tree. If no
 * snapshot is available yet, it shows a placeholder.
 *
 * The upcoming-tasks list is populated with [RemoteViewsCompat.RemoteCollectionItems]
 * instead of a [RemoteViewsService], so the list is updated directly as part
 * of the [RemoteViews] tree and no longer depends on
 * [AppWidgetManager.notifyAppWidgetViewDataChanged].
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

            val snapshot = snapshotJson?.let { JSONObject(it) }
            if (snapshot != null) {
                renderSnapshot(views, snapshot, updatedAt)
            } else {
                renderPlaceholder(views)
            }

            // Pass the collection items directly into the RemoteViews. This avoids the
            // RemoteViewsService / notifyAppWidgetViewDataChanged path that can get stuck
            // in the "loading" state on newer Android versions.
            val items = buildCollectionItems(context, snapshot)
            RemoteViewsCompat.setRemoteAdapter(
                context,
                views,
                widgetId,
                R.id.widget_upcoming_list,
                items,
            )

            // Template PendingIntent for per-item clicks. Each item fills in its own
            // data URI (takusu://task/<id>) via setOnClickFillInIntent. The template
            // must NOT set its own data field, otherwise Intent.fillIn() will not
            // override it.
            val templateIntent =
                Intent(Intent.ACTION_VIEW).apply {
                    flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP
                    // Make the intent explicit; Android 14+ rejects mutable PendingIntents
                    // with implicit intents.
                    setPackage(context.packageName)
                }
            val templatePi =
                PendingIntent.getActivity(
                    context,
                    0,
                    templateIntent,
                    PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                )
            views.setPendingIntentTemplate(R.id.widget_upcoming_list, templatePi)

            // Tap on the widget root opens the app.
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
            // No notifyAppWidgetViewDataChanged is needed because the RemoteViews
            // now carries the full collection of items.
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

private fun buildCollectionItems(
    context: Context,
    snapshot: JSONObject?,
): RemoteViewsCompat.RemoteCollectionItems {
    val builder =
        RemoteViewsCompat.RemoteCollectionItems
            .Builder()
            .setViewTypeCount(1)
            .setHasStableIds(false)
    val upcoming = snapshot?.optJSONArray("upcoming") ?: JSONArray()
    for (i in 0 until upcoming.length()) {
        val task = upcoming.optJSONObject(i) ?: continue
        builder.addItem(i.toLong(), buildItemRemoteViews(context, task))
    }
    return builder.build()
}

private fun buildItemRemoteViews(
    context: Context,
    task: JSONObject,
): RemoteViews {
    val views = RemoteViews(context.packageName, R.layout.takusu_widget_item)

    val title = task.optString("title", "")
    val startAt = if (task.isNull("start_at")) null else task.optString("start_at", null)
    val time = if (startAt != null) formatTime(startAt) else ""
    val text = if (time.isNotEmpty()) "$time  $title" else title
    views.setTextViewText(R.id.widget_item_text, text)

    val taskId = task.optString("id", "")
    val fillIn = Intent(Intent.ACTION_VIEW, Uri.parse("takusu://task/$taskId"))
    views.setOnClickFillInIntent(R.id.widget_item_text, fillIn)

    return views
}

private val ISO_FRACTIONAL_SECONDS = Regex("\\.\\d+")

private fun formatTime(iso: String): String =
    try {
        val s = iso.replace(ISO_FRACTIONAL_SECONDS, "").replace("Z", "+00:00")
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

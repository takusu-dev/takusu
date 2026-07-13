package expo.modules.takusuwidget

import android.content.Context
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.records.Field
import expo.modules.kotlin.records.Record
import org.json.JSONArray
import org.json.JSONObject

class WidgetConfig : Record {
    @Field val workersUrl: String = ""

    @Field val token: String = ""
}

class WidgetUpcomingTask : Record {
    @Field val id: String = ""

    @Field val title: String = ""

    @Field val startAt: String? = null

    @Field val endAt: String = ""
}

class WidgetSnapshotInput : Record {
    @Field val doingTitles: List<String> = emptyList()

    @Field val upcoming: List<WidgetUpcomingTask> = emptyList()

    @Field val unscheduledCount: Int = 0
}

/**
 * Expo module that bridges JS → native for the home screen widget.
 *
 * JS calls `saveConfig` to persist the Workers URL + token into
 * SharedPreferences (read by [WidgetUpdateWorker]).
 *
 * JS calls `saveSnapshot` to write the latest task data (fetched on the
 * JS side) into SharedPreferences and immediately refresh the widget,
 * so the widget shows fresh data without waiting for WorkManager.
 *
 * `requestUpdate` triggers a widget re-render from the cached snapshot.
 */
class TakusuWidgetModule : Module() {
    override fun definition() =
        ModuleDefinition {
            Name("TakusuWidget")

            Function("saveConfig") { config: WidgetConfig ->
                try {
                    val ctx = appContext.reactContext ?: return@Function false
                    val prefs = ctx.getSharedPreferences(WidgetUpdateWorker.PREFS_NAME, Context.MODE_PRIVATE)
                    prefs
                        .edit()
                        .putString(WidgetUpdateWorker.KEY_WORKERS_URL, config.workersUrl)
                        .putString(WidgetUpdateWorker.KEY_TOKEN, config.token)
                        .apply()
                    // Schedule periodic updates as soon as credentials are available.
                    WidgetUpdateWorker.schedule(ctx)
                    true
                } catch (e: Exception) {
                    false
                }
            }

            Function("saveSnapshot") { input: WidgetSnapshotInput ->
                try {
                    val ctx = appContext.reactContext ?: return@Function false
                    val prefs = ctx.getSharedPreferences(WidgetUpdateWorker.PREFS_NAME, Context.MODE_PRIVATE)

                    val arr = JSONArray()
                    for (task in input.upcoming) {
                        val o = JSONObject()
                        o.put("id", task.id)
                        o.put("title", task.title)
                        if (task.startAt !=
                            null
                        ) {
                            o.put("start_at", task.startAt)
                        } else {
                            o.put("start_at", JSONObject.NULL)
                        }
                        o.put("end_at", task.endAt)
                        arr.put(o)
                    }
                    val snap = JSONObject()
                    val doingArr = JSONArray()
                    for (title in input.doingTitles) {
                        doingArr.put(title)
                    }
                    snap.put("doing_titles", doingArr)
                    snap.put("upcoming", arr)
                    snap.put("unscheduled_count", input.unscheduledCount)

                    prefs
                        .edit()
                        .putString(WidgetUpdateWorker.KEY_SNAPSHOT, snap.toString())
                        .putLong(WidgetUpdateWorker.KEY_UPDATED_AT, System.currentTimeMillis())
                        .apply()

                    TakusuWidgetProvider.updateWidget(ctx)
                    true
                } catch (e: Exception) {
                    false
                }
            }

            Function("requestUpdate") {
                try {
                    val ctx = appContext.reactContext ?: return@Function false
                    TakusuWidgetProvider.updateWidget(ctx)
                    true
                } catch (e: Exception) {
                    false
                }
            }
        }
}

package expo.modules.takusuwidget

import android.content.Context
import androidx.core.content.ContextCompat

internal object WidgetDotColors {
    fun color(
        context: Context,
        task: UpcomingTask,
    ): Int {
        val colorRes =
            when {
                task.fixed -> R.color.takusu_widget_brand
                task.abandonability < 0.25 -> R.color.takusu_widget_must
                task.abandonability < 0.5 -> R.color.takusu_widget_caution
                else -> R.color.takusu_widget_calm
            }
        return ContextCompat.getColor(context, colorRes)
    }
}

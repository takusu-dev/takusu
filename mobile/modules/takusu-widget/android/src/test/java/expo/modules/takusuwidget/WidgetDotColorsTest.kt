package expo.modules.takusuwidget

import android.os.Build
import androidx.core.content.ContextCompat
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [Build.VERSION_CODES.UPSIDE_DOWN_CAKE])
class WidgetDotColorsTest {
    private val context = RuntimeEnvironment.getApplication()

    private fun task(
        fixed: Boolean = false,
        abandonability: Double = 0.75,
    ) = UpcomingTask(
        id = "1",
        title = "test",
        startAt = null,
        endAt = "",
        abandonability = abandonability,
        fixed = fixed,
    )

    @Test
    fun fixedTaskUsesBrandColor() {
        assertEquals(
            ContextCompat.getColor(context, R.color.takusu_widget_brand),
            WidgetDotColors.color(context, task(fixed = true)),
        )
    }

    @Test
    fun lowAbandonabilityUsesMustColor() {
        assertEquals(
            ContextCompat.getColor(context, R.color.takusu_widget_must),
            WidgetDotColors.color(context, task(abandonability = 0.0)),
        )
        assertEquals(
            ContextCompat.getColor(context, R.color.takusu_widget_must),
            WidgetDotColors.color(context, task(abandonability = 0.24)),
        )
    }

    @Test
    fun mediumAbandonabilityUsesCautionColor() {
        assertEquals(
            ContextCompat.getColor(context, R.color.takusu_widget_caution),
            WidgetDotColors.color(context, task(abandonability = 0.25)),
        )
        assertEquals(
            ContextCompat.getColor(context, R.color.takusu_widget_caution),
            WidgetDotColors.color(context, task(abandonability = 0.49)),
        )
    }

    @Test
    fun highAbandonabilityUsesCalmColor() {
        assertEquals(
            ContextCompat.getColor(context, R.color.takusu_widget_calm),
            WidgetDotColors.color(context, task(abandonability = 0.5)),
        )
        assertEquals(
            ContextCompat.getColor(context, R.color.takusu_widget_calm),
            WidgetDotColors.color(context, task(abandonability = 0.75)),
        )
    }
}

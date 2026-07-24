package expo.modules.takusuappicon

import android.app.Activity
import android.content.Context
import android.content.SharedPreferences

private const val PREFS_NAME = "takusu_theme_prefs"
private const val KEY_THEME = "takusu_theme"
private const val DEFAULT_THEME = "light"

object TakusuTheme {
    private val themeToStyle =
        mapOf(
            "light" to R.style.TakusuLight,
            "dark" to R.style.TakusuDark,
            "catppuccin" to R.style.TakusuCatppuccin,
            "aura-soft-dark" to R.style.TakusuAuraSoftDark,
        )

    private fun getPreferences(context: Context): SharedPreferences =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    fun saveTheme(
        context: Context,
        theme: String,
    ) {
        getPreferences(context)
            .edit()
            .putString(KEY_THEME, theme)
            .apply()
    }

    fun getSavedTheme(context: Context): String =
        getPreferences(context).getString(KEY_THEME, DEFAULT_THEME) ?: DEFAULT_THEME

    fun apply(activity: Activity) {
        val theme = getSavedTheme(activity)
        val style = themeToStyle[theme] ?: R.style.TakusuLight
        activity.setTheme(style)
    }
}

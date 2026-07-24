package expo.modules.takusuappicon

import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

class TakusuAppIconModule : Module() {
    override fun definition() =
        ModuleDefinition {
            Name("TakusuAppIcon")

            Function("getAvailableThemes") {
                THEMES.keys.toList()
            }

            Function("setTheme") { theme: String ->
                val context =
                    appContext.reactContext
                        ?: throw CodedException("ERR_NO_CONTEXT", "React context is unavailable", null)
                val normalized = normalizeTheme(theme)
                TakusuTheme.saveTheme(context, normalized)
                applyAppIcon(context, normalized)
                true
            }
        }

    private fun applyAppIcon(
        context: Context,
        theme: String,
    ) {
        val packageName = context.packageName
        val pm = context.packageManager

        THEMES.forEach { (slug, alias) ->
            val component = ComponentName(packageName, "$packageName.$alias")
            val state =
                if (slug == theme) {
                    PackageManager.COMPONENT_ENABLED_STATE_ENABLED
                } else {
                    PackageManager.COMPONENT_ENABLED_STATE_DISABLED
                }
            pm.setComponentEnabledSetting(
                component,
                state,
                PackageManager.DONT_KILL_APP,
            )
        }
    }

    companion object {
        private val THEMES =
            linkedMapOf(
                "light" to "MainActivityLight",
                "dark" to "MainActivityDark",
                "catppuccin" to "MainActivityCatppuccin",
                "aura-soft-dark" to "MainActivityAuraSoftDark",
            )

        private fun normalizeTheme(theme: String): String =
            THEMES.keys.find { it == theme.lowercase() }
                ?: "light"
    }
}

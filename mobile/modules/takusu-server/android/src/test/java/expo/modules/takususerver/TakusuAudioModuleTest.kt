package expo.modules.takususerver

import android.os.Build
import android.speech.tts.TextToSpeech
import android.speech.tts.Voice
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [Build.VERSION_CODES.UPSIDE_DOWN_CAKE])
class TakusuAudioModuleTest {
    private val context = RuntimeEnvironment.getApplication()

    private val voiceA = Voice("ja-jp-x-abc", Locale.JAPAN, 400, 200, false, emptySet())
    private val voiceB = Voice("en-us-x-def", Locale.US, 400, 200, false, emptySet())

    private lateinit var tts: FakeTextToSpeech

    @Before
    fun setUp() {
        tts = FakeTextToSpeech(context)
        tts.voiceSet = mutableSetOf(voiceB, voiceA)
    }

    @Test
    fun `voicesFromTts returns sorted voice maps`() {
        val result = voicesFromTts(tts)

        assertEquals(2, result.size)
        assertEquals("en-us-x-def", result[0]["name"])
        assertEquals("en-US", result[0]["locale"])
        assertEquals(400, result[0]["quality"])
        assertEquals(200, result[0]["latency"])
        assertEquals(false, result[0]["requiresNetworkConnection"])
        assertEquals(emptyList<String>(), result[0]["features"])
        assertEquals("ja-jp-x-abc", result[1]["name"])
    }

    @Test
    fun `applyTtsOptions selects first sorted voice when voiceId is empty`() {
        applyTtsOptions(tts, voiceId = "", language = "ja", speed = 1.2f)

        assertEquals("en-us-x-def", tts.selectedVoice?.name)
        assertEquals(1.2f, tts.currentSpeechRate)
    }

    @Test
    fun `applyTtsOptions selects matching voice by name`() {
        applyTtsOptions(tts, voiceId = "ja-jp-x-abc", language = "ja", speed = 1.0f)

        assertEquals("ja-jp-x-abc", tts.selectedVoice?.name)
    }

    @Test
    fun `applyTtsOptions leaves voice unchanged when setVoice fails`() {
        tts.setVoiceResult = TextToSpeech.ERROR

        applyTtsOptions(tts, voiceId = "ja-jp-x-abc", language = "ja", speed = 1.0f)

        assertEquals(voiceA, tts.selectedVoice)
    }

    @Test
    fun `applyTtsOptions falls back to default locale when language is unsupported`() {
        tts.setLanguageResult = TextToSpeech.LANG_MISSING_DATA

        applyTtsOptions(tts, voiceId = "", language = "unknown", speed = 1.0f)

        assertEquals(Locale.getDefault(), tts.currentLocale)
    }

    private class FakeTextToSpeech(
        context: android.content.Context,
    ) : TextToSpeech(context, {}) {
        var voiceSet: MutableSet<Voice> = mutableSetOf()
        var selectedVoice: Voice? = null
        var currentLocale: Locale? = null
        var currentSpeechRate: Float = 1.0f
        var setVoiceResult: Int = TextToSpeech.SUCCESS
        var setLanguageResult: Int = TextToSpeech.SUCCESS

        override fun getVoices(): MutableSet<Voice> = voiceSet

        override fun setVoice(voice: Voice?): Int {
            selectedVoice = voice
            return setVoiceResult
        }

        override fun setLanguage(locale: Locale?): Int {
            currentLocale = locale
            return setLanguageResult
        }

        override fun setSpeechRate(rate: Float): Int {
            currentSpeechRate = rate
            return TextToSpeech.SUCCESS
        }
    }
}

package expo.modules.takususerver

import android.content.Context
import android.media.AudioAttributes
import android.media.MediaPlayer
import android.speech.tts.TextToSpeech
import android.speech.tts.Voice
import android.util.Log
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.functions.Coroutine
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.records.Field
import expo.modules.kotlin.records.Record
import java.io.File
import java.util.Locale
import java.util.UUID
import java.util.concurrent.atomic.AtomicReference
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlin.coroutines.suspendCoroutine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.takusu_android.MobileAudio

class AudioOptions : Record {
    @Field val provider: String = "cartesia"

    @Field val modelDir: String = ""

    @Field val apiKey: String = ""

    @Field val voiceId: String = ""

    @Field val language: String = "ja"

    @Field val sampleRate: Int = 44100

    @Field val speed: Double = 1.0
}

private const val TAG = "TakusuAudioModule"

class TakusuAudioModule : Module() {
    private var audio: MobileAudio? = null
    private var recorder: AudioRecorder? = null
    private var player: MediaPlayer? = null
    private var textToSpeech: TextToSpeech? = null
    private var ttsProvider: String = "cartesia"
    private var recordingSampleRate: Int = 0

    override fun definition() =
        ModuleDefinition {
            Name("TakusuAudio")

            AsyncFunction("configure") Coroutine { options: AudioOptions ->
                val context =
                    appContext.reactContext
                        ?: throw CodedException("ERR_AUDIO_CONFIG", "React context is not available", null)

                // Stop any active playback and release the previous backend
                // resources before switching.
                player?.stop()
                player?.release()
                player = null
                try {
                    audio?.shutdown()
                } catch (_: Exception) {
                    // ignore shutdown failures
                }
                audio = null
                textToSpeech?.shutdown()
                textToSpeech = null

                // Reset the provider choice; it will be restored only after the
                // new backend initializes successfully.
                ttsProvider = ""

                when (options.provider) {
                    "android" -> {
                        val newAudio = createMobileAudio(context, options, apiKey = "")
                        val tts =
                            try {
                                initTextToSpeech(context)
                            } catch (error: Exception) {
                                try {
                                    newAudio.shutdown()
                                } catch (_: Exception) {
                                    // ignore shutdown failures
                                }
                                throw error
                            }
                        applyTtsOptions(tts, options)
                        textToSpeech = tts
                        audio = newAudio
                    }

                    "cartesia" -> {
                        audio = createMobileAudio(context, options, apiKey = options.apiKey)
                    }

                    else -> {
                        throw CodedException(
                            "ERR_TTS_UNSUPPORTED",
                            "Unsupported TTS provider: ${options.provider}",
                            null,
                        )
                    }
                }

                ttsProvider = options.provider
                true
            }

            Function("startRecording") {
                if (recordingSampleRate <= 0) {
                    throw CodedException(
                        "ERR_AUDIO_CONFIG",
                        "Audio is not configured with a valid sample rate",
                        null,
                    )
                }
                val instance = AudioRecorder(recordingSampleRate)
                instance.start()
                recorder = instance
                true
            }

            AsyncFunction("stopAndTranscribe") {
                val samples =
                    recorder?.stop()
                        ?: throw CodedException("ERR_NOT_RECORDING", "Recording is not active", null)
                recorder = null
                val instance =
                    audio
                        ?: throw CodedException("ERR_AUDIO_CONFIG", "Audio is not configured", null)
                instance.transcribePcm(samples)
            }

            AsyncFunction("synthesizeAndPlay") { text: String ->
                if (text.trim().isEmpty()) {
                    throw CodedException("ERR_TTS_EMPTY", "TTS text was empty", null)
                }

                if (ttsProvider.isEmpty()) {
                    throw CodedException(
                        "ERR_AUDIO_CONFIG",
                        "TTS provider is not configured",
                        null,
                    )
                }

                if (ttsProvider == "android") {
                    val tts =
                        textToSpeech
                            ?: throw CodedException("ERR_AUDIO_CONFIG", "Android TTS is not configured", null)
                    val result = tts.speak(text, TextToSpeech.QUEUE_FLUSH, null, UUID.randomUUID().toString())
                    if (result == TextToSpeech.ERROR) {
                        throw CodedException("ERR_TTS_FAILED", "Android TTS speak failed", null)
                    }
                    true
                } else {
                    val instance =
                        audio
                            ?: throw CodedException("ERR_AUDIO_CONFIG", "Audio is not configured", null)
                    val mp3 = instance.synthesize(text)
                    val cacheDir =
                        appContext.reactContext?.cacheDir
                            ?: throw CodedException("ERR_AUDIO_CONFIG", "React context is not available", null)
                    val file = File(cacheDir, "takusu-agent-response.mp3")
                    file.writeBytes(mp3)
                    player?.release()
                    player =
                        MediaPlayer().also { mediaPlayer ->
                            mediaPlayer.setAudioAttributes(
                                AudioAttributes
                                    .Builder()
                                    .setUsage(AudioAttributes.USAGE_MEDIA)
                                    .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                                    .build(),
                            )
                            mediaPlayer.setDataSource(file.absolutePath)
                            mediaPlayer.setOnCompletionListener { it.release() }
                            mediaPlayer.prepare()
                            mediaPlayer.start()
                        }
                    true
                }
            }

            Function("stopPlayback") {
                if (ttsProvider == "android") {
                    textToSpeech?.stop()
                } else {
                    player?.stop()
                    player?.release()
                    player = null
                }
                true
            }
        }

    private suspend fun initTextToSpeech(context: Context): TextToSpeech =
        withContext(Dispatchers.Main) {
            suspendCoroutine { continuation ->
                val ttsRef = AtomicReference<TextToSpeech>()
                val tts =
                    TextToSpeech(context.applicationContext) { status ->
                        if (status == TextToSpeech.SUCCESS) {
                            val instance = ttsRef.get()
                            if (instance != null) {
                                continuation.resume(instance)
                            } else {
                                continuation.resumeWithException(
                                    CodedException(
                                        "ERR_TTS_INIT",
                                        "Android TTS initialized before reference was set",
                                        null,
                                    ),
                                )
                            }
                        } else {
                            continuation.resumeWithException(
                                CodedException(
                                    "ERR_TTS_INIT",
                                    "Android TTS initialization failed with status $status",
                                    null,
                                ),
                            )
                        }
                    }
                ttsRef.set(tts)
            }
        }

    private fun createMobileAudio(
        context: Context,
        options: AudioOptions,
        apiKey: String,
    ): MobileAudio {
        val modelDir =
            options.modelDir.ifEmpty {
                File(context.noBackupFilesDir, "takusu/models").absolutePath
            }
        recordingSampleRate = options.sampleRate
        return try {
            MobileAudio(
                modelDir,
                apiKey,
                options.voiceId,
                options.language,
                options.sampleRate.toUInt(),
                options.speed.toFloat(),
            )
        } catch (error: Exception) {
            throw CodedException(
                "ERR_AUDIO_CONFIG",
                "Failed to load audio models: ${error.message}",
                error,
            )
        }
    }

    private fun applyTtsOptions(
        tts: TextToSpeech,
        options: AudioOptions,
    ) {
        val locale = Locale.forLanguageTag(options.language)
        val languageResult = tts.setLanguage(locale)
        if (languageResult == TextToSpeech.LANG_MISSING_DATA ||
            languageResult == TextToSpeech.LANG_NOT_SUPPORTED
        ) {
            Log.w(TAG, "TTS language '${options.language}' is not supported, falling back to default locale")
            tts.setLanguage(Locale.getDefault())
        }

        if (options.voiceId.isNotEmpty()) {
            val voice: Voice? = tts.voices?.find { it.name == options.voiceId }
            if (voice != null) {
                tts.voice = voice
            } else {
                Log.w(TAG, "TTS voice '${options.voiceId}' not found, using default")
            }
        }

        tts.setSpeechRate(options.speed.toFloat())
    }
}

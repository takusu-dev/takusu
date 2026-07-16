package expo.modules.takususerver

import android.media.AudioAttributes
import android.media.MediaPlayer
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import expo.modules.kotlin.records.Field
import expo.modules.kotlin.records.Record
import java.io.File
import uniffi.takusu_android.MobileAudio

class AudioOptions : Record {
    @Field val modelDir: String = ""

    @Field val apiKey: String = ""

    @Field val voiceId: String = ""

    @Field val language: String = "ja"

    @Field val sampleRate: Int = 44100
}

class TakusuAudioModule : Module() {
    private var audio: MobileAudio? = null
    private var recorder: AudioRecorder? = null
    private var player: MediaPlayer? = null

    override fun definition() =
        ModuleDefinition {
            Name("TakusuAudio")

            AsyncFunction("configure") { options: AudioOptions ->
                val modelDir =
                    options.modelDir.ifEmpty {
                        appContext.reactContext?.noBackupFilesDir?.absolutePath
                            ?: throw CodedException("ERR_AUDIO_CONFIG", "React context is not available", null)
                    }
                audio =
                    try {
                        MobileAudio(
                            modelDir,
                            options.apiKey,
                            options.voiceId,
                            options.language,
                            options.sampleRate.toUInt(),
                        )
                    } catch (error: Exception) {
                        throw CodedException("ERR_AUDIO_CONFIG", "Failed to load audio models: ${error.message}", error)
                    }
                true
            }

            Function("startRecording") {
                val instance = AudioRecorder()
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
                val instance =
                    audio
                        ?: throw CodedException("ERR_AUDIO_CONFIG", "Audio is not configured", null)
                val wav = instance.synthesizeWav(text)
                val cacheDir =
                    appContext.reactContext?.cacheDir
                        ?: throw CodedException("ERR_AUDIO_CONFIG", "React context is not available", null)
                val file = File(cacheDir, "takusu-agent-response.wav")
                file.writeBytes(wav)
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

            Function("stopPlayback") {
                player?.stop()
                player?.release()
                player = null
                true
            }
        }
}

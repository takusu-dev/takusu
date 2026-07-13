package expo.modules.takususerver

import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import java.util.Collections
import java.util.concurrent.atomic.AtomicBoolean

class AudioRecorder {
    private val running = AtomicBoolean(false)
    private val samples = Collections.synchronizedList(mutableListOf<Short>())
    private var recorder: AudioRecord? = null
    private var thread: Thread? = null

    fun start() {
        check(running.compareAndSet(false, true)) { "recording is already running" }
        val minimumBuffer =
            AudioRecord.getMinBufferSize(
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
            )
        check(minimumBuffer > 0) { "microphone is unavailable" }
        val audioRecord =
            AudioRecord(
                MediaRecorder.AudioSource.VOICE_RECOGNITION,
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                minimumBuffer * 2,
            )
        check(audioRecord.state == AudioRecord.STATE_INITIALIZED) { "failed to initialize microphone" }
        samples.clear()
        recorder = audioRecord
        thread =
            Thread {
                val buffer = ShortArray(minimumBuffer)
                try {
                    audioRecord.startRecording()
                    while (running.get() && samples.size < SAMPLE_RATE * MAX_DURATION_SECONDS) {
                        val count = audioRecord.read(buffer, 0, buffer.size)
                        if (count > 0) {
                            synchronized(samples) {
                                for (index in 0 until count) samples.add(buffer[index])
                            }
                        }
                    }
                } finally {
                    audioRecord.stop()
                    audioRecord.release()
                }
            }.also { it.start() }
    }

    fun stop(): List<Short> {
        check(running.compareAndSet(true, false)) { "recording is not running" }
        thread?.join()
        thread = null
        recorder = null
        return synchronized(samples) { samples.toList() }
    }

    companion object {
        const val SAMPLE_RATE = 16_000
        private const val MAX_DURATION_SECONDS = 60
    }
}

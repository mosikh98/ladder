package com.mosikh98.ladder

import android.os.Handler
import android.os.Looper
import java.util.concurrent.atomic.AtomicBoolean

/** In-memory log + run state shared by the service and the UI. */
object LadderLog {
    private val lines = java.util.ArrayDeque<String>()
    private val main = Handler(Looper.getMainLooper())
    private val pending = AtomicBoolean(false)

    @Volatile
    var running = false
        private set

    @Volatile
    var label = ""
        private set

    @Volatile
    var listener: (() -> Unit)? = null

    fun begin(newLabel: String) {
        synchronized(lines) { lines.clear() }
        label = newLabel
        running = true
        changed()
    }

    fun end() {
        running = false
        changed()
    }

    fun add(line: String) {
        synchronized(lines) {
            lines.addLast(line)
            while (lines.size > 800) lines.removeFirst()
        }
        changed()
    }

    fun snapshot(): String = synchronized(lines) { lines.joinToString("\n") }

    private fun changed() {
        if (pending.compareAndSet(false, true)) {
            main.postDelayed({
                pending.set(false)
                listener?.invoke()
            }, 200)
        }
    }
}

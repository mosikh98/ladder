package com.mosikh98.ladder

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder
import android.os.PowerManager

class LadderService : Service() {
    companion object {
        const val ACTION_START = "com.mosikh98.ladder.START"
        const val ACTION_STOP = "com.mosikh98.ladder.STOP"
        const val EXTRA_ARGS = "args"
        const val EXTRA_LABEL = "label"
        private const val CHANNEL = "ladder"
        private const val NOTIF_ID = 1
    }

    @Volatile
    private var proc: Process? = null
    private var wake: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START -> startRun(intent)
            ACTION_STOP -> stopRun()
            else -> stopSelf()
        }
        return START_NOT_STICKY
    }

    private fun startRun(intent: Intent) {
        val label = intent.getStringExtra(EXTRA_LABEL) ?: "ladder"
        val args = intent.getStringArrayExtra(EXTRA_ARGS) ?: emptyArray()
        startForeground(NOTIF_ID, buildNotification(label))
        if (proc != null) return

        LadderLog.begin(label)
        LadderLog.add("$ ladder " + args.joinToString(" "))
        try {
            Env.prepare(this)
            val bin = Env.ladderBin(this)
            if (!bin.exists()) {
                LadderLog.add("libladder.so پیدا نشد: " + bin.path)
                finishRun()
                return
            }
            val cmd = ArrayList<String>()
            cmd.add(bin.path)
            cmd.add("--config")
            cmd.add(Env.file(this, Env.CONFIG).path)
            cmd.addAll(args)
            val pb = ProcessBuilder(cmd)
                .directory(Env.workDir(this))
                .redirectErrorStream(true)
            pb.environment()["HOME"] = filesDir.path
            val p = pb.start()
            proc = p
            acquireWake()
            Thread {
                try {
                    p.inputStream.bufferedReader().forEachLine { LadderLog.add(it) }
                } catch (e: Exception) {
                    // stream closed
                }
                val code = try {
                    p.waitFor()
                } catch (e: Exception) {
                    -1
                }
                LadderLog.add("--- exit $code ---")
                if (proc === p) proc = null
                finishRun()
            }.start()
        } catch (e: Exception) {
            LadderLog.add("خطا: " + e.message)
            finishRun()
        }
    }

    private fun stopRun() {
        val p = proc
        if (p == null) {
            finishRun()
            return
        }
        p.destroy() // SIGTERM: ladder kills its psiphon child and exits
        Thread {
            try {
                Thread.sleep(3000)
            } catch (e: InterruptedException) {
                // ignore
            }
            if (p.isAlive) p.destroyForcibly()
        }.start()
    }

    private fun finishRun() {
        LadderLog.end()
        releaseWake()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun acquireWake() {
        val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
        val w = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "ladder:run")
        w.setReferenceCounted(false)
        w.acquire()
        wake = w
    }

    private fun releaseWake() {
        val w = wake
        if (w != null && w.isHeld) w.release()
        wake = null
    }

    private fun buildNotification(label: String): Notification {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL, "Ladder", NotificationManager.IMPORTANCE_LOW)
        )
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE
        )
        val stop = PendingIntent.getService(
            this, 1,
            Intent(this, LadderService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("نردبان")
            .setContentText(label)
            .setContentIntent(open)
            .setOngoing(true)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, "توقف", stop)
            .build()
    }

    override fun onDestroy() {
        proc?.destroy()
        releaseWake()
        super.onDestroy()
    }
}

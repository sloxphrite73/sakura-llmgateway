package dev.sakura.llmgateway

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.atomic.AtomicReference

/**
 * Foreground service that owns the gateway subprocess lifecycle:
 *
 *  - spawns the bundled native binary (renamed libllmgateway.so in jniLibs,
 *    extracted to nativeLibraryDir by the package manager) with
 *    `--config <filesDir>/gateway.json`
 *  - runs a 10s HTTP watchdog against the gateway UI port (`/api/status`),
 *    restarting the process if it dies *or* hangs
 *  - START_STICKY so Android recreates the service (and thus the gateway)
 *    after the system kills the app process
 *  - graceful stop: onDestroy asks the process to terminate and waits briefly
 *    so stats.json / gateway.json debounced writes land on disk
 */
class GatewayService : Service() {

    companion object {
        const val CHANNEL_ID = "gateway"
        const val NOTIF_ID = 1
        const val ACTION_STOP = "dev.sakura.llmgateway.STOP"
        const val API_PORT = 8000L
        const val UI_PORT = 8001L
        const val WATCHDOG_INTERVAL_MS = 10_000L
        const val WATCHDOG_TIMEOUT_MS = 5_000L

        @Volatile var isRunning: Boolean = false
            private set

        /** Live state for the UI: "running" / "starting" / "stopped". */
        private val state = AtomicReference("stopped")
        fun currentState(): String = state.get()

        fun start(context: Context) {
            val i = Intent(context, GatewayService::class.java)
            if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(i) else context.startService(i)
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, GatewayService::class.java))
        }
    }

    private var process: Process? = null
    @Volatile private var watchdogThread: Thread? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
        val n = buildNotification()
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(NOTIF_ID, n, android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(NOTIF_ID, n)
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        state.set("starting")
        spawnGateway()
        startWatchdog()
        isRunning = true
        return START_STICKY
    }

    private fun gatewayBinary(): File {
        // AGP renames jniLibs/<abi>/libllmgateway.so into nativeLibraryDir.
        return File(applicationInfo.nativeLibraryDir, "libllmgateway.so")
    }

    private fun spawnGateway() {
        val bin = gatewayBinary()
        if (!bin.exists()) {
            state.set("stopped")
            android.util.Log.e("GatewayService", "gateway binary missing at ${bin.absolutePath}")
            return
        }
        val config = File(filesDir, "gateway.json")
        try {
            process = ProcessBuilder(bin.absolutePath, "--config", config.absolutePath)
                .directory(filesDir)
                .redirectErrorStream(true)
                .start()
            // Drain stdout/stderr into logcat so failures are diagnosable.
            val p = process!!
            Thread {
                p.inputStream.bufferedReader().useLines { lines ->
                    lines.forEach { android.util.Log.i("gateway", it) }
                }
            }.start()
            state.set("running")
        } catch (e: Exception) {
            android.util.Log.e("GatewayService", "spawn failed", e)
            state.set("stopped")
        }
    }

    private fun startWatchdog() {
        watchdogThread?.interrupt()
        watchdogThread = Thread {
            while (!Thread.currentThread().isInterrupted) {
                try {
                    Thread.sleep(WATCHDOG_INTERVAL_MS)
                } catch (_: InterruptedException) {
                    return@Thread
                }
                val alive = process?.isAlive == true && healthy()
                if (!alive) {
                    android.util.Log.w("GatewayService", "watchdog: gateway unhealthy, restarting")
                    killProcess()
                    state.set("starting")
                    spawnGateway()
                }
            }
        }.apply { isDaemon = true; start() }
    }

    /** GET /api/status on the UI port; true only on HTTP 200 within the timeout. */
    private fun healthy(): Boolean {
        return try {
            val conn = URL("http://127.0.0.1:$UI_PORT/api/status").openConnection() as HttpURLConnection
            conn.connectTimeout = WATCHDOG_TIMEOUT_MS.toInt()
            conn.readTimeout = WATCHDOG_TIMEOUT_MS.toInt()
            val ok = conn.responseCode == 200
            conn.disconnect()
            ok
        } catch (_: Exception) {
            false
        }
    }

    private fun killProcess() {
        process?.let { p ->
            try {
                p.destroy()
                if (!p.waitFor(3, java.util.concurrent.TimeUnit.SECONDS)) p.destroyForcibly()
            } catch (_: Exception) {}
        }
        process = null
    }

    private fun createChannel() {
        val ch = NotificationChannel(CHANNEL_ID, getString(R.string.notif_channel), NotificationManager.IMPORTANCE_LOW)
        (getSystemService(NOTIFICATION_SERVICE) as NotificationManager).createNotificationChannel(ch)
    }

    private fun buildNotification(): Notification {
        val pi = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        val stopPi = PendingIntent.getService(
            this, 1,
            Intent(this, GatewayService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        return Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.stat_notify_sync_noanim) // neutral system icon
            .setContentTitle(getString(R.string.notif_title))
            .setContentText(getString(R.string.notif_text))
            .setContentIntent(pi)
            .addAction(Notification.Action.Builder(null, "停止", stopPi).build())
            .setOngoing(true)
            .build()
    }

    override fun onDestroy() {
        isRunning = false
        watchdogThread?.interrupt()
        watchdogThread = null
        // Graceful: let the gateway flush stats.json / debounced config writes.
        killProcess()
        state.set("stopped")
        super.onDestroy()
    }
}

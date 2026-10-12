package dev.sakura.llmgateway

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import java.io.File

/**
 * Relaunch the gateway's foreground service after a device reboot, but only
 * when the user has a config (i.e. finished onboarding) — a fresh install that
 * never set up should stay quiet.
 *
 * On MIUI/HyperOS this receiver only fires when the OEM "autostart" permission
 * is granted (Settings → Apps → Sakura → 自启动); the console's keep-alive card
 * deep-links the user there. Stock Android fires it unconditionally for a
 * manifest-declared BOOT_COMPLETED receiver; starting a specialUse foreground
 * service from boot is allowed.
 */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
        if (!File(context.filesDir, "gateway.json").exists()) return
        android.util.Log.i("BootReceiver", "boot completed; starting gateway service")
        GatewayService.start(context)
    }
}

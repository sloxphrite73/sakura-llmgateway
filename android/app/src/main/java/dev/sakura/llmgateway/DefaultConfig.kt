package dev.sakura.llmgateway

import android.content.Context
import java.io.File

/**
 * Minimal valid gateway.json for the "skip onboarding" path.
 * Matches the Rust config schema (llm-gateway/src/config.rs):
 * `providers` is EMPTY — no placeholder provider, so the console starts clean
 * and users add their own providers/keys in the UI. An empty provider list is a
 * first-class runtime state (it's exactly what `Config::default()` produces when
 * the gateway's config file is missing).
 */
object DefaultConfig {
    const val JSON = """{
  "api_port": 8000,
  "ui_port": 8001,
  "default_cooldown_secs": 60,
  "max_attempts": 3,
  "auth": { "enabled": false, "keys": [] },
  "providers": []
}"""

    fun writeIfMissing(ctx: Context) {
        val f = File(ctx.filesDir, "gateway.json")
        if (!f.exists()) f.writeText(JSON)
    }
}

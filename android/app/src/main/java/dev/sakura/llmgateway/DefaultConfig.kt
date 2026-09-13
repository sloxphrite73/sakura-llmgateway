package dev.sakura.llmgateway

import android.content.Context
import java.io.File

/**
 * Minimal valid gateway.json for the "skip onboarding" path.
 * Matches the Rust config schema (llm-gateway/src/config.rs):
 * `providers` is a non-empty array with one placeholder provider and no keys,
 * so the gateway starts, the console works, and real keys get added in the UI.
 */
object DefaultConfig {
    const val JSON = """{
  "api_port": 8000,
  "ui_port": 8001,
  "default_cooldown_secs": 60,
  "max_attempts": 3,
  "auth": { "enabled": false, "keys": [] },
  "providers": [
    {
      "id": "sensenova",
      "name": "sensenova",
      "base_url": "https://token.sensenova.cn/v1",
      "keys": [],
      "models": [],
      "model_allowlist_only": false,
      "aliases": {}
    }
  ]
}"""

    fun writeIfMissing(ctx: Context) {
        val f = File(ctx.filesDir, "gateway.json")
        if (!f.exists()) f.writeText(JSON)
    }
}

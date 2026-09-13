package dev.sakura.llmgateway

import android.app.Activity
import android.app.DownloadManager
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.view.View
import android.webkit.CookieManager
import android.webkit.DownloadListener
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * Single-activity app:
 *  - no gateway.json in filesDir -> Compose onboarding: import one (file picker)
 *    or skip with a generated empty default config
 *  - config present -> fullscreen WebView console at the gateway UI port,
 *    with the gateway started as a foreground service
 *
 * Also handles ACTION_VIEW for gateway.json from file managers ("open with"):
 * the file is copied into filesDir and hot-reloaded via /api/config/import.
 */
class MainActivity : Activity() {

    companion object {
        const val UI_URL = "http://127.0.0.1:8001"
        val SAKURA = Color(0xFFE88FA2)
    }

    private val compose = mutableStateOf(0)

    // ---- launchers -----------------------------------------------------------

    private val importConfig =
        registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
            if (uri != null) importFromUri(uri, hotReloadIfRunning = false)
        }

    private val exportDoc =
        registerForActivityResult(ActivityResultContracts.CreateDocument("application/json")) { uri: Uri? ->
            if (uri != null) saveExport(uri)
        }

    // ---- lifecycle -----------------------------------------------------------

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        handleOpenWith(intent)
        setContent { Root() }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleOpenWith(intent)
    }

    // ---- open-with gateway.json ---------------------------------------------

    private fun handleOpenWith(intent: Intent?) {
        if (intent?.action != Intent.ACTION_VIEW) return
        val uri = intent.data ?: return
        importFromUri(uri, hotReloadIfRunning = true)
    }

    private fun importFromUri(uri: Uri, hotReloadIfRunning: Boolean) {
        val ctx = this
        kotlinx.coroutines.CoroutineScope(Dispatchers.IO).launch {
            val ok = runCatching {
                val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@runCatching false
                val tmp = File(ctx.filesDir, "gateway.json.incoming")
                tmp.writeBytes(bytes)
                if (!validConfig(bytes)) {
                    tmp.delete(); return@runCatching false
                }
                val dst = File(ctx.filesDir, "gateway.json")
                if (hotReloadIfRunning && GatewayService.isRunning) {
                    // Hot-reload through the gateway's own validate-then-swap endpoint.
                    val okHttp = postImport(bytes)
                    if (okHttp) tmp.renameTo(dst) // gateway already persisted equivalent config; keep ours aligned
                } else {
                    tmp.renameTo(dst)
                }
                tmp.delete()
                true
            }.getOrDefault(false)
            withContext(Dispatchers.Main) {
                android.widget.Toast.makeText(
                    ctx,
                    if (ok) "配置已导入" else "导入失败：不是有效的 gateway.json",
                    android.widget.Toast.LENGTH_LONG
                ).show()
                if (ok) {
                    compose.value += 1 // re-evaluate: config now exists
                    if (GatewayService.isRunning.not()) GatewayService.start(ctx)
                }
            }
        }
    }

    private fun validConfig(bytes: ByteArray): Boolean {
        return try {
            val obj = org.json.JSONObject(String(bytes, Charsets.UTF_8))
            obj.has("providers") && obj.get("providers") is org.json.JSONArray
        } catch (_: Exception) {
            false
        }
    }

    private fun postImport(bytes: ByteArray): Boolean {
        return try {
            val conn = URL("http://127.0.0.1:8001/api/config/import").openConnection() as HttpURLConnection
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.setRequestProperty("Content-Type", "application/json")
            conn.setFixedLengthStreamingMode(bytes.size)
            conn.outputStream.use { it.write(bytes) }
            val ok = conn.responseCode in 200..299
            conn.disconnect()
            ok
        } catch (_: Exception) {
            false
        }
    }

    // ---- SAF export ----------------------------------------------------------

    private fun saveExport(uri: Uri) {
        val ctx = this
        kotlinx.coroutines.CoroutineScope(Dispatchers.IO).launch {
            val ok = runCatching {
                val bytes = fetchExport()
                contentResolver.openOutputStream(uri)?.use { it.write(bytes) } ?: return@runCatching false
                true
            }.getOrDefault(false)
            withContext(Dispatchers.Main) {
                android.widget.Toast.makeText(
                    ctx,
                    if (ok) "已保存" else "导出失败",
                    android.widget.Toast.LENGTH_SHORT
                ).show()
            }
        }
    }

    private fun fetchExport(): ByteArray {
        val conn = URL("http://127.0.0.1:8001/api/config/export").openConnection() as HttpURLConnection
        conn.connectTimeout = 5000
        conn.readTimeout = 10000
        val data = conn.inputStream.use { it.readBytes() }
        conn.disconnect()
        return data
    }

    // ---- UI ------------------------------------------------------------------

    @Composable
    fun Root() {
        // re-read on compose.value change
        val tick = compose.value
        val hasConfig = remember(tick) { File(filesDir, "gateway.json").exists() }
        if (hasConfig) Console() else Onboarding()
    }

    @Composable
    fun Onboarding() {
        val ctx = LocalContext.current
        val scope = rememberCoroutineScope()
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(
                Modifier.fillMaxSize().padding(32.dp),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                Text("🌸", style = MaterialTheme.typography.displayLarge)
                Spacer(Modifier.height(16.dp))
                Text(
                    "Sakura LLM Gateway",
                    style = MaterialTheme.typography.headlineMedium,
                    textAlign = TextAlign.Center
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    "手机上的 OpenAI 兼容网关。\n导入你电脑上导出的 gateway.json 即可开始；\n也可以先空着启动，之后在控制台里添加。",
                    style = MaterialTheme.typography.bodyMedium,
                    textAlign = TextAlign.Center,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
                Spacer(Modifier.height(32.dp))
                Button(
                    onClick = { importConfig.launch(arrayOf("application/json", "application/octet-stream", "*/*")) },
                    modifier = Modifier.fillMaxWidth()
                ) { Text("导入 gateway.json") }
                Spacer(Modifier.height(12.dp))
                OutlinedButton(
                    onClick = {
                        scope.launch(Dispatchers.IO) {
                            DefaultConfig.writeIfMissing(ctx)
                            withContext(Dispatchers.Main) {
                                compose.value += 1
                                GatewayService.start(ctx)
                            }
                        }
                    },
                    modifier = Modifier.fillMaxWidth()
                ) { Text("跳过，用空配置启动") }
            }
        }
    }

    @Composable
    fun Console() {
        val ctx = LocalContext.current
        DisposableEffect(Unit) {
            GatewayService.start(ctx)
            onDispose { /* keep service running when activity recreated */ }
        }
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { context ->
                WebView(context).apply {
                    settings.javaScriptEnabled = true
                    settings.domStorageEnabled = true
                    webViewClient = object : WebViewClient() {
                        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                            // external links open in the browser
                            return if (request.url.host != "127.0.0.1") {
                                context.startActivity(Intent(Intent.ACTION_VIEW, request.url))
                                true
                            } else false
                        }
                    }
                    // /api/config/export -> SAF save dialog
                    setDownloadListener(DownloadListener { url, _, contentDisposition, mimeType, _ ->
                        if (url.contains("/api/config/export")) {
                            exportDoc.launch("gateway.json")
                        } else {
                            val req = DownloadManager.Request(Uri.parse(url))
                                .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                            (getSystemService(DOWNLOAD_SERVICE) as DownloadManager).enqueue(req)
                        }
                        @Suppress("UNUSED_EXPRESSION")
                        contentDisposition
                        mimeType
                    })
                    loadUrl(UI_URL)
                }
            },
            update = { it.loadUrl(UI_URL) }
        )
    }

    override fun onBackPressed() {
        // WebView history stays internal to the console; default behavior is fine.
        super.onBackPressed()
    }
}

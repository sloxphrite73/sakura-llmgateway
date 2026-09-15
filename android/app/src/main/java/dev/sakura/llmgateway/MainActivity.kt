package dev.sakura.llmgateway

import android.app.Activity
import android.app.AlertDialog
import android.app.DownloadManager
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.view.View
import android.webkit.CookieManager
import android.webkit.DownloadListener
import android.webkit.JsResult
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import kotlinx.coroutines.delay
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
class MainActivity : androidx.activity.ComponentActivity() {

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

    // WebView <input type=file> support (WebChromeClient.onShowFileChooser -> SAF picker).
    // Without this, the console's 导入配置 button does nothing on Android.
    private var fileChooserCallback: ValueCallback<Array<Uri>>? = null

    private val fileChooser =
        registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
            val cb = fileChooserCallback
            fileChooserCallback = null
            cb?.onReceiveValue(uri?.let { arrayOf(it) })
        }

    // ---- lifecycle -----------------------------------------------------------

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestNotificationPermissionIfNeeded()
        handleOpenWith(intent)
        setContent { Root() }
    }

    /**
     * POST_NOTIFICATIONS is a runtime permission since Android 13 (API 33). Without
     * requesting it, the foreground-service notification (and its 停止 action) never
     * shows even though the service itself runs. The service still works without it;
     * this only controls notification visibility.
     */
    private fun requestNotificationPermissionIfNeeded() {
        if (android.os.Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) !=
            android.content.pm.PackageManager.PERMISSION_GRANTED
        ) {
            requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1001)
        }
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
            // Issue #2 (small screens, e.g. 441x537): a fixed-height centered Column
            // squeezes the two action buttons when content exceeds the viewport —
            // the buttons shrink below their 48dp minimum tap target and become
            // effectively untappable. scrollable + vertically-centered-when-it-fits:
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 32.dp, vertical = 24.dp),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                Spacer(Modifier.navigationBarsPadding())
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
                    modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)
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
                    modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)
                ) { Text("跳过，用空配置启动") }
                Spacer(Modifier.navigationBarsPadding())
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
        // Track gateway health so we can show a retry screen instead of WebView's
        // dead "ERR_CONNECTION_REFUSED" page while the process is still booting.
        // NOTE: the health probe does real socket I/O, so it MUST run on Dispatchers.IO.
        // Doing it on the main thread throws NetworkOnMainThreadException, which the
        // catch swallowed -> "false" forever -> the app was stuck on "网关启动中…".
        var gatewayUp by remember { mutableStateOf(GatewayService.currentState() == "running") }
        LaunchedEffect(Unit) {
            while (true) {
                gatewayUp = withContext(Dispatchers.IO) { isGatewayUp() }
                if (gatewayUp) break
                kotlinx.coroutines.delay(500)
            }
        }
        if (!gatewayUp) {
            // Scrollable for the same small-screen reason as Onboarding (issue #2).
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 32.dp, vertical = 24.dp),
                verticalArrangement = Arrangement.Center,
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                CircularProgressIndicator()
                Spacer(Modifier.height(16.dp))
                Text("网关启动中…", style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.height(8.dp))
                Text(
                    "正在等待 127.0.0.1:8001 就绪（通常不到一秒）。\n若长时间停留在此页面，请下拉通知栏用「停止」后重开 app。",
                    style = MaterialTheme.typography.bodySmall,
                    textAlign = TextAlign.Center,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
            return
        }
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { context ->
                WebView(context).apply {
                    settings.javaScriptEnabled = true
                    settings.domStorageEnabled = true
                    webChromeClient = object : WebChromeClient() {
                        // file input (导入配置 in the console)
                        override fun onShowFileChooser(
                            webView: WebView,
                            callback: ValueCallback<Array<Uri>>,
                            params: FileChooserParams
                        ): Boolean {
                            fileChooserCallback?.onReceiveValue(null) // cancel any pending one
                            fileChooserCallback = callback
                            fileChooser.launch(arrayOf("application/json", "application/octet-stream", "*/*"))
                            return true
                        }

                        // JS alert()/confirm() used by the console (import confirmation, toasts)
                        override fun onJsAlert(view: WebView, url: String, message: String, result: JsResult): Boolean {
                            AlertDialog.Builder(context)
                                .setMessage(message)
                                .setPositiveButton("确定") { _, _ -> result.confirm() }
                                .setOnCancelListener { result.cancel() }
                                .show()
                            return true
                        }

                        override fun onJsConfirm(view: WebView, url: String, message: String, result: JsResult): Boolean {
                            AlertDialog.Builder(context)
                                .setMessage(message)
                                .setPositiveButton("确定") { _, _ -> result.confirm() }
                                .setNegativeButton("取消") { _, _ -> result.cancel() }
                                .setOnCancelListener { result.cancel() }
                                .show()
                            return true
                        }
                    }
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
            update = { view ->
                // Only (re)load when not already on the console URL — reloading on every
                // recomposition would wipe the user's console state mid-interaction.
                if (view.url != UI_URL) view.loadUrl(UI_URL)
            }
        )
    }

    /** GET /api/status with a short timeout — used to gate the WebView on gateway readiness. */
    private fun isGatewayUp(): Boolean {
        return try {
            val conn = URL("http://127.0.0.1:8001/api/status").openConnection() as HttpURLConnection
            conn.connectTimeout = 1500
            conn.readTimeout = 1500
            val ok = conn.responseCode == 200
            conn.disconnect()
            ok
        } catch (_: Exception) {
            false
        }
    }

    override fun onBackPressed() {
        // WebView history stays internal to the console; default behavior is fine.
        super.onBackPressed()
    }
}

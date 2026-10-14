package de.touchdeck.app

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.ActivityInfo
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.view.Gravity
import android.view.View
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import android.webkit.JavascriptInterface
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.window.OnBackInvokedDispatcher
import de.touchdeck.app.Ui.button
import de.touchdeck.app.Ui.dp
import de.touchdeck.app.Ui.text
import kotlin.concurrent.thread

/**
 * Das Bedienfeld im Vollbild. Es wird vom PC geladen und ist damit immer auf dem Stand der PC-Version.
 * Die App ergänzt Vollbild, wach bleibenden Bildschirm, Vibration und die Zurück-Taste.
 */
class DeckActivity : Activity() {
    private lateinit var prefs: Prefs
    private lateinit var server: Server
    private lateinit var web: WebView
    private lateinit var offline: LinearLayout
    private lateinit var offlineText: android.widget.TextView
    private val main = Handler(Looper.getMainLooper())
    private var failed = false
    private val retry = Runnable { load() }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = Prefs(this)
        server = prefs.current ?: return backToConnect(null)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        web = WebView(this).apply {
            setBackgroundColor(Ui.BG)
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.mediaPlaybackRequiresUserGesture = false
            settings.setSupportZoom(false)
            settings.builtInZoomControls = false
            overScrollMode = View.OVER_SCROLL_NEVER
            isHapticFeedbackEnabled = false
            addJavascriptInterface(Bridge(), "TouchDeckApp")
            webViewClient = DeckClient()
        }
        offlineText = text("", 16f, Ui.TEXT).apply { gravity = Gravity.CENTER }
        offline = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            setBackgroundColor(Ui.BG)
            setPadding(dp(32), dp(32), dp(32), dp(32))
            visibility = View.GONE
            addView(text("📡", 44f).apply { gravity = Gravity.CENTER })
            addView(offlineText, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(12); bottomMargin = dp(20) })
            addView(button("Erneut versuchen", primary = true) { load() })
            addView(button("Verbindung ändern") { backToConnect(null) }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(10) })
        }
        setContentView(FrameLayout(this).apply {
            addView(web)
            addView(offline)
        })

        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { goBack() }
        }
        load()
        checkPairing()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) immersive()
    }

    /** Status- und Navigationsleiste ausblenden; mit Wischen vom Rand kurz einblendbar. */
    private fun immersive() {
        if (Build.VERSION.SDK_INT >= 30) {
            window.insetsController?.let {
                it.hide(WindowInsets.Type.statusBars() or WindowInsets.Type.navigationBars())
                it.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = (View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or View.SYSTEM_UI_FLAG_FULLSCREEN
                or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN)
        }
    }

    private fun load() {
        main.removeCallbacks(retry)
        failed = false
        web.loadUrl(server.deckUrl())
    }

    /** Ist der Schlüssel am PC inzwischen ungültig (z. B. neu erzeugt), muss neu gekoppelt werden. */
    private fun checkPairing() {
        thread {
            try {
                val r = Net.checkToken(server)
                if (r.status == 401 && !r.body.optBoolean("pin")) {
                    prefs.forgetToken(server)
                    runOnUiThread { backToConnect("Die Kopplung mit „${server.name}“ gilt nicht mehr (z. B. neuer Schlüssel am PC). Bitte neu verbinden.") }
                }
            } catch (_: Exception) {
                // Nicht erreichbar – das zeigt die Seite selbst an.
            }
        }
    }

    private fun showOffline() {
        offlineText.text = "„${server.name}“ ist gerade nicht erreichbar.\nLäuft TouchDeck am PC, und ist das Handy im selben WLAN?\n\nNeuer Versuch in wenigen Sekunden …"
        offline.visibility = View.VISIBLE
        main.postDelayed(retry, 4000)
    }

    private fun goBack() {
        // Erst im Bedienfeld eine Seite zurück; ganz am Anfang die App in den Hintergrund legen.
        web.evaluateJavascript("window.deckBack ? window.deckBack() : false") { result ->
            if (result != "true") moveTaskToBack(true)
        }
    }

    @Deprecated("Für Android 12 und älter")
    override fun onBackPressed() {
        goBack()
    }

    /** Zurück zum Verbindungsbildschirm (ohne gleich wieder hierher weiterzuleiten). */
    private fun backToConnect(message: String?) {
        startActivity(Intent(this, ConnectActivity::class.java).apply {
            putExtra(ConnectActivity.EXTRA_MANUAL, true)
            if (message != null) putExtra(ConnectActivity.EXTRA_MESSAGE, message)
        })
        finish()
    }

    override fun onDestroy() {
        main.removeCallbacks(retry)
        if (::web.isInitialized) web.destroy()
        super.onDestroy()
    }

    private inner class DeckClient : WebViewClient() {
        override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
            if (request.isForMainFrame) {
                failed = true
                showOffline()
            }
        }

        override fun onPageFinished(view: WebView, url: String) {
            if (!failed) offline.visibility = View.GONE
        }

        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            val url = request.url
            if (url.host == server.host) return false
            // Fremde Links im normalen Browser öffnen.
            startActivity(Intent(Intent.ACTION_VIEW, url))
            return true
        }
    }

    /** Wird aus dem Bedienfeld aufgerufen (window.TouchDeckApp). */
    inner class Bridge {
        @JavascriptInterface
        fun vibrate(ms: Int) {
            val duration = ms.coerceIn(1, 500).toLong()
            val vibrator = if (Build.VERSION.SDK_INT >= 31) {
                (getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as VibratorManager).defaultVibrator
            } else {
                @Suppress("DEPRECATION")
                getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            }
            vibrator.vibrate(VibrationEffect.createOneShot(duration, VibrationEffect.DEFAULT_AMPLITUDE))
        }

        /** Ausrichtung aus den TouchDeck-Einstellungen: auto (folgt der Drehsperre), portrait, landscape. */
        @JavascriptInterface
        fun setOrientation(mode: String) {
            runOnUiThread {
                requestedOrientation = when (mode) {
                    "portrait" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT
                    "landscape" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                    else -> ActivityInfo.SCREEN_ORIENTATION_FULL_USER
                }
            }
        }

        @JavascriptInterface
        fun openConnection() {
            runOnUiThread { backToConnect(null) }
        }
    }
}

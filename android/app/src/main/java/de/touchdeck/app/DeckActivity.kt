package de.touchdeck.app

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.ActivityInfo
import android.os.Build
import android.provider.Settings
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.view.Gravity
import android.view.View
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
import de.touchdeck.app.Ui.immersive
import de.touchdeck.app.Ui.text
import kotlin.concurrent.thread

/**
 * Das Bedienfeld im Vollbild. Es wird vom PC geladen und ist damit immer auf dem Stand der PC-Version.
 * Die App ergänzt Vollbild, wach bleibenden Bildschirm, Vibration und die Zurück-Taste.
 */
class DeckActivity : Activity() {
    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(Lang.wrap(newBase))

    private lateinit var prefs: Prefs
    private lateinit var server: Server
    private lateinit var web: WebView
    private lateinit var offline: LinearLayout
    private lateinit var offlineText: android.widget.TextView
    private lateinit var retryButton: android.widget.TextView
    private lateinit var changeButton: android.widget.TextView
    private val main = Handler(Looper.getMainLooper())
    private var failed = false
    private val retry = Runnable { load() }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = Prefs(this)
        server = prefs.current ?: return backToConnect(null)
        device = DeviceSettings(this)
        applyScreen()

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
            retryButton = button(getString(R.string.retry), primary = true) { load() }
            changeButton = button(getString(R.string.change_connection)) { backToConnect(null) }
            addView(retryButton)
            addView(changeButton, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(10) })
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
                    runOnUiThread { backToConnect(getString(R.string.pairing_invalid, server.name)) }
                }
            } catch (_: Exception) {
                // Nicht erreichbar – das zeigt die Seite selbst an.
            }
        }
    }

    private fun showOffline() {
        offlineText.text = Lang.wrap(this).getString(R.string.offline, server.name)
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

    // Im Hintergrund: Seite gilt als versteckt (Animationen aus); das Bedienfeld meldet sich dann
    // beim PC ab, damit er keine Live-Werte mehr schickt. Kein pauseTimers(), sonst käme die Abmeldung
    // erst beim Zurückkommen an.
    override fun onPause() {
        watchMotion(false)
        if (::web.isInitialized) web.onPause()
        super.onPause()
    }

    override fun onResume() {
        super.onResume()
        applyScreen()
        if (::web.isInitialized) {
            web.onResume()
            // Geräte-Einstellungen könnten sich geändert haben (Einstellungen-Bildschirm).
            web.evaluateJavascript("window.tdDeviceSettingsChanged && window.tdDeviceSettingsChanged()", null)
        }
    }

    // ---------- Geräte-Einstellungen: Bildschirm anlassen, Helligkeit, Bewegung im Ruhezustand ----------

    private lateinit var device: DeviceSettings
    private var dimmed = false

    /** Bildschirm anlassen und Helligkeit (Betrieb bzw. Ruhezustand) nach den Geräte-Einstellungen. */
    private fun applyScreen() {
        if (!::device.isInitialized) return
        if (device.keepOn) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        window.attributes = window.attributes.apply {
            screenBrightness = when {
                dimmed -> device.sleepBrightness / 100f
                device.brightness >= 0 -> device.brightness / 100f
                else -> WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE
            }
        }
    }

    private val motion = object : android.hardware.SensorEventListener {
        private var last: FloatArray? = null
        private var lastFired = 0L
        override fun onSensorChanged(e: android.hardware.SensorEvent) {
            val prev = last
            last = e.values.clone()
            if (prev == null) return
            val delta = Math.abs(e.values[0] - prev[0]) + Math.abs(e.values[1] - prev[1]) + Math.abs(e.values[2] - prev[2])
            val now = System.currentTimeMillis()
            // Spürbar bewegt (nicht nur Rauschen), höchstens alle 2 Sekunden melden.
            if (delta > 1.5f && now - lastFired > 2000) {
                lastFired = now
                web.evaluateJavascript("window.tdMotion && window.tdMotion()", null)
            }
        }
        override fun onAccuracyChanged(sensor: android.hardware.Sensor?, accuracy: Int) {}
    }

    /** Beschleunigungssensor nur im Ruhezustand und nur, wenn eingeschaltet (spart Akku). */
    private fun watchMotion(on: Boolean) {
        val sm = getSystemService(Context.SENSOR_SERVICE) as android.hardware.SensorManager
        sm.unregisterListener(motion)
        if (on && ::device.isInitialized && device.motion) {
            sm.getDefaultSensor(android.hardware.Sensor.TYPE_ACCELEROMETER)?.let { sm.registerListener(motion, it, android.hardware.SensorManager.SENSOR_DELAY_NORMAL) }
        }
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
            // Webseiten-Buttons: eingebettete Seiten (Rahmen) bleiben im Button.
            if (!request.isForMainFrame) return false
            if (url.host == server.host) return false
            // Fremde Links im normalen Browser öffnen; ohne passendes Programm nichts tun statt abzustürzen.
            try {
                startActivity(Intent(Intent.ACTION_VIEW, url))
            } catch (e: Exception) {
                // z. B. ActivityNotFoundException bei unbekannten Schemata
            }
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

        /** Ruhezustand des Bedienfelds: Bildschirm fast dunkel bzw. wieder normale Helligkeit. */
        @JavascriptInterface
        fun setDim(dim: Boolean) {
            runOnUiThread {
                dimmed = dim
                applyScreen()
                watchMotion(dim)
            }
        }

        /** Nur die Einstellungen, die auf diesem Gerät anders sein sollen als am PC (JSON). */
        @JavascriptInterface
        fun deviceSettings(): String = device.toJson()

        /** Akzentfarbe von Android (Material You) als #rrggbb, leer ohne. */
        @JavascriptInterface
        fun systemAccent(): String = DeviceSettings.systemAccent(this@DeckActivity)

        /** Einstellungen dieses Geräts öffnen (Fußleiste im Bedienfeld). */
        @JavascriptInterface
        fun openSettings() {
            runOnUiThread { startActivity(Intent(this@DeckActivity, DeviceSettingsActivity::class.java)) }
        }

        /** Gerätename (wie in den Android-Einstellungen) für die Statusleiste am PC. */
        @JavascriptInterface
        fun deviceName(): String =
            Settings.Global.getString(contentResolver, "device_name")?.takeIf { it.isNotBlank() } ?: "${Build.MANUFACTURER} ${Build.MODEL}"

        /** Sprache aus den TouchDeck-Einstellungen; gilt dann auch für den Verbinden-Bildschirm. */
        @JavascriptInterface
        fun setLanguage(lang: String) {
            if (lang !in Lang.SUPPORTED || lang == prefs.language) return
            prefs.language = lang
            runOnUiThread {
                val texts = Lang.wrap(this@DeckActivity)
                retryButton.text = texts.getString(R.string.retry)
                changeButton.text = texts.getString(R.string.change_connection)
            }
        }

        @JavascriptInterface
        fun openConnection() {
            runOnUiThread { backToConnect(null) }
        }
    }
}

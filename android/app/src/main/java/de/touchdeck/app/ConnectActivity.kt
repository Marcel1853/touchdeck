// Der QR-Scanner wird über IntentIntegrator gestartet; die neuere Variante bräuchte AndroidX Activity.
@file:Suppress("DEPRECATION")

package de.touchdeck.app

import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.text.InputType
import android.view.Gravity
import android.view.View
import android.view.WindowInsets
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.TextView
import com.google.zxing.integration.android.IntentIntegrator
import de.touchdeck.app.Ui.button
import de.touchdeck.app.Ui.dp
import de.touchdeck.app.Ui.pressable
import de.touchdeck.app.Ui.rounded
import de.touchdeck.app.Ui.text
import kotlin.concurrent.thread

/**
 * Startbildschirm: gespeicherte PCs, im WLAN gefundene PCs, QR-Code und Adresse von Hand.
 * Ist ein PC als aktuell gespeichert, geht es direkt zum Bedienfeld.
 */
class ConnectActivity : Activity() {
    private lateinit var prefs: Prefs
    private lateinit var savedList: LinearLayout
    private lateinit var savedTitle: TextView
    private lateinit var foundList: LinearLayout
    private lateinit var status: TextView
    private val found = linkedMapOf<String, Server>()
    private val discovery = Discovery { addFound(it) }
    @Volatile private var pairing: String? = null

    /** Sprache, in der der Bildschirm aufgebaut wurde. */
    private var shownLanguage: String? = null

    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(Lang.wrap(newBase))

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = Prefs(this)
        shownLanguage = prefs.language
        if (prefs.current != null && !intent.getBooleanExtra(EXTRA_MANUAL, false)) {
            openDeck()
            return
        }
        setContentView(buildUi())
        renderSaved()
        intent.getStringExtra(EXTRA_MESSAGE)?.let { showMessage(it) }
    }

    override fun onResume() {
        super.onResume()
        // Inzwischen andere Sprache am PC eingestellt: Bildschirm in der neuen Sprache aufbauen.
        if (::foundList.isInitialized && prefs.language != shownLanguage) return recreate()
        if (::foundList.isInitialized) discovery.start()
    }

    override fun onPause() {
        super.onPause()
        discovery.stop()
    }

    // ---------- Aufbau ----------

    private fun buildUi(): View {
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24), dp(28), dp(24), dp(28))
        }
        column.addView(LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(TextView(context).apply {
                text = "▦"
                textSize = 22f
                setTextColor(android.graphics.Color.WHITE)
                gravity = Gravity.CENTER
                background = rounded(Ui.ACCENT, 12)
                layoutParams = LinearLayout.LayoutParams(dp(44), dp(44)).apply { rightMargin = dp(12) }
            })
            addView(text("TouchDeck", 26f, bold = true))
        })
        column.addView(text(getString(R.string.connect_hint), 15f, Ui.MUTED).apply {
            setPadding(0, dp(14), 0, dp(10))
        })

        savedTitle = sectionTitle(getString(R.string.my_pcs))
        column.addView(savedTitle)
        savedList = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(savedList)

        column.addView(sectionTitle(getString(R.string.found_in_wifi)))
        status = text(getString(R.string.searching), 14f, Ui.MUTED)
        column.addView(LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(ProgressBar(context).apply { isIndeterminate = true; layoutParams = LinearLayout.LayoutParams(dp(22), dp(22)).apply { rightMargin = dp(10) } })
            addView(status)
            setPadding(0, 0, 0, dp(10))
        })
        foundList = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(foundList)

        column.addView(sectionTitle(getString(R.string.or)))
        column.addView(button(getString(R.string.scan_qr)) { scanQr() }, fullWidth())
        column.addView(button(getString(R.string.enter_address)) { manualDialog() }, fullWidth(top = 10))
        column.addView(text(getString(R.string.where_on_pc), 13f, Ui.MUTED).apply {
            setPadding(0, dp(18), 0, 0)
        })

        val scroll = ScrollView(this).apply {
            setBackgroundColor(Ui.BG)
            isFillViewport = true
            addView(column)
        }
        // Platz für Statusleiste und Navigationsleiste lassen (Android zeichnet sonst darunter).
        scroll.setOnApplyWindowInsetsListener { v, insets ->
            if (Build.VERSION.SDK_INT >= 30) {
                val bars = insets.getInsets(WindowInsets.Type.systemBars())
                v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            } else {
                @Suppress("DEPRECATION")
                v.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            }
            insets
        }
        return scroll
    }

    private fun sectionTitle(label: String) = text(label.uppercase(), 13f, Ui.MUTED, bold = true).apply {
        letterSpacing = 0.06f
        setPadding(0, dp(20), 0, dp(10))
    }

    private fun fullWidth(top: Int = 0) = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(top) }

    private fun card(title: String, subtitle: String, onClick: () -> Unit, onLongClick: (() -> Unit)? = null) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        background = pressable(Ui.PANEL, 16, Ui.LINE)
        setPadding(dp(18), dp(16), dp(18), dp(16))
        isClickable = true
        addView(text(title, 18f, bold = true))
        addView(text(subtitle, 13f, Ui.MUTED).apply { setPadding(0, dp(4), 0, 0) })
        setOnClickListener { onClick() }
        if (onLongClick != null) setOnLongClickListener { onLongClick(); true }
    }

    // ---------- Gespeicherte PCs ----------

    private fun renderSaved() {
        savedList.removeAllViews()
        val saved = prefs.servers
        savedTitle.visibility = if (saved.isEmpty()) View.GONE else View.VISIBLE
        for (server in saved) {
            val sub = "${server.host}:${server.port} " + getString(if (server.isPaired) R.string.paired else R.string.needs_pairing)
            savedList.addView(card("🖥️  ${server.name}", sub, { connectSaved(server) }, { savedMenu(server) }), fullWidth(top = if (savedList.childCount > 0) 10 else 0))
        }
        if (saved.isNotEmpty()) savedList.addView(text(getString(R.string.long_press_hint), 12f, Ui.MUTED).apply { setPadding(dp(4), dp(8), 0, 0) })
        // Gefundene, die schon gespeichert sind, nicht doppelt zeigen.
        renderFound()
    }

    private fun connectSaved(server: Server) {
        if (server.isPaired) {
            prefs.use(server)
            openDeck()
        } else {
            pair(server)
        }
    }

    private fun savedMenu(server: Server) {
        AlertDialog.Builder(this)
            .setTitle(server.name)
            .setItems(arrayOf(getString(R.string.rename), getString(R.string.pair_again), getString(R.string.remove))) { _, which ->
                when (which) {
                    0 -> renameDialog(server)
                    1 -> pair(server)
                    2 -> AlertDialog.Builder(this)
                        .setMessage(getString(R.string.remove_question, server.name))
                        .setPositiveButton(R.string.remove) { _, _ -> prefs.remove(server); renderSaved() }
                        .setNegativeButton(R.string.cancel, null)
                        .show()
                }
            }
            .show()
    }

    private fun renameDialog(server: Server) {
        val input = EditText(this).apply { setText(server.name); setSelection(server.name.length) }
        AlertDialog.Builder(this)
            .setTitle(R.string.pc_name)
            .setView(LinearLayout(this).apply { setPadding(dp(22), dp(8), dp(22), 0); addView(input, fullWidth()) })
            .setPositiveButton(R.string.save) { _, _ ->
                val name = input.text.toString().trim()
                if (name.isNotEmpty()) prefs.rename(server, name)
                renderSaved()
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    // ---------- Gefundene PCs ----------

    private fun addFound(server: Server) {
        // Noch nie verbunden: die Sprache des ersten gefundenen PCs übernehmen.
        if (prefs.language == null && server.language in Lang.SUPPORTED) {
            prefs.language = server.language
            if (server.language != shownLanguage) return recreate()
        }
        if (found.containsKey(server.key)) return
        found[server.key] = server
        renderFound()
    }

    private fun renderFound() {
        if (!::foundList.isInitialized) return
        val savedKeys = prefs.servers.map { it.key }.toSet()
        val fresh = found.values.filter { it.key !in savedKeys }
        foundList.removeAllViews()
        for (server in fresh) {
            val sub = "${server.host}:${server.port}" + if (server.version.isNotEmpty()) " · TouchDeck ${server.version}" else ""
            foundList.addView(card("🖥️  ${server.name}", getString(R.string.tap_to_pair, sub), { pair(server) }), fullWidth(top = if (foundList.childCount > 0) 10 else 0))
        }
        status.text = when {
            fresh.isNotEmpty() -> if (fresh.size == 1) getString(R.string.one_new_pc) else getString(R.string.new_pcs, fresh.size)
            found.isNotEmpty() -> getString(R.string.no_more_pcs)
            else -> getString(R.string.searching)
        }
    }

    // ---------- Koppeln ----------

    private fun deviceName(): String =
        Settings.Global.getString(contentResolver, "device_name")?.takeIf { it.isNotBlank() } ?: "${Build.MANUFACTURER} ${Build.MODEL}"

    /** Kopplung anfragen und warten, bis am PC „Erlauben“ gedrückt wird. */
    private fun pair(server: Server) {
        val dialog = AlertDialog.Builder(this)
            .setTitle(R.string.confirm_on_pc)
            .setMessage(getString(R.string.confirm_on_pc_text, server.name, deviceName()))
            .setNegativeButton(R.string.cancel) { _, _ -> pairing = null }
            .setCancelable(false)
            .show()
        thread {
            var cancelled = false
            try {
                val id = Net.requestPairing(server, deviceName())
                pairing = id
                val until = System.currentTimeMillis() + 120_000
                while (System.currentTimeMillis() < until) {
                    Thread.sleep(1000)
                    if (pairing != id) {
                        cancelled = true
                        return@thread
                    }
                    val st = Net.pairingStatus(server, id)
                    when (st.optString("state")) {
                        "approved" -> {
                            prefs.save(server.copy(token = st.getString("token")))
                            runOnUiThread { dialog.dismiss(); openDeck() }
                            return@thread
                        }
                        "denied" -> throw IllegalStateException(getString(R.string.denied))
                        "expired" -> throw IllegalStateException(getString(R.string.expired))
                    }
                }
                throw IllegalStateException(getString(R.string.no_answer))
            } catch (e: Exception) {
                if (!cancelled) runOnUiThread {
                    dialog.dismiss()
                    showMessage(friendly(e))
                }
            } finally {
                pairing = null
            }
        }
    }

    private fun friendly(e: Exception): String = when (e) {
        is java.net.ConnectException, is java.net.SocketTimeoutException, is java.net.NoRouteToHostException ->
            getString(R.string.pc_not_responding)
        is Net.PcError -> getString(R.string.pc_error_code, e.status)
        else -> e.message ?: getString(R.string.connection_failed)
    }

    private fun manualDialog() {
        val last = prefs.servers.firstOrNull()
        val host = EditText(this).apply { hint = getString(R.string.ip_hint); inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI; setText(last?.host ?: "") }
        val port = EditText(this).apply { hint = getString(R.string.port); inputType = InputType.TYPE_CLASS_NUMBER; setText((last?.port ?: Prefs.DEFAULT_PORT).toString()) }
        AlertDialog.Builder(this)
            .setTitle(R.string.enter_address_title)
            .setView(LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(dp(22), dp(8), dp(22), 0)
                addView(host)
                addView(port)
            })
            .setPositiveButton(R.string.connect) { _, _ ->
                val h = host.text.toString().trim().removePrefix("http://").substringBefore('/').substringBefore(':')
                val p = port.text.toString().toIntOrNull() ?: Prefs.DEFAULT_PORT
                if (h.isNotEmpty()) pair(prefs.servers.find { it.host == h && it.port == p } ?: Server(h, p))
            }
            .setNegativeButton(R.string.cancel, null)
            .show()
    }

    // ---------- QR-Code ----------

    private fun scanQr() {
        IntentIntegrator(this)
            .setDesiredBarcodeFormats(IntentIntegrator.QR_CODE)
            .setPrompt(getString(R.string.scan_prompt))
            .setBeepEnabled(false)
            .setOrientationLocked(false)
            .initiateScan()
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        val result = IntentIntegrator.parseActivityResult(requestCode, resultCode, data)
        if (result == null) {
            super.onActivityResult(requestCode, resultCode, data)
            return
        }
        val contents = result.contents ?: return
        // Erwartet: http://IP:PORT/deck/?token=…
        val uri = Uri.parse(contents)
        val host = uri.host
        val token = uri.getQueryParameter("token")
        if (host.isNullOrEmpty() || token.isNullOrEmpty()) {
            showMessage(getString(R.string.not_touchdeck_qr))
            return
        }
        val port = if (uri.port > 0) uri.port else Prefs.DEFAULT_PORT
        val known = prefs.servers.find { it.host == host && it.port == port }
        val name = known?.name ?: found["$host:$port"]?.name ?: host
        prefs.save(Server(host, port, name, token = token))
        openDeck()
    }

    // ---------- Sonstiges ----------

    private fun showMessage(message: String) {
        AlertDialog.Builder(this).setMessage(message).setPositiveButton("OK", null).show()
    }

    private fun openDeck() {
        startActivity(Intent(this, DeckActivity::class.java))
        finish()
    }

    companion object {
        const val EXTRA_MANUAL = "manual"
        const val EXTRA_MESSAGE = "message"
    }
}

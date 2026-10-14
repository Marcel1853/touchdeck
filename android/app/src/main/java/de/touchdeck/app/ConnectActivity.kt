// Der QR-Scanner wird über IntentIntegrator gestartet; die neuere Variante bräuchte AndroidX Activity.
@file:Suppress("DEPRECATION")

package de.touchdeck.app

import android.app.Activity
import android.app.AlertDialog
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = Prefs(this)
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
        column.addView(text("Handy und PC müssen im selben WLAN sein, und TouchDeck muss am PC laufen.", 15f, Ui.MUTED).apply {
            setPadding(0, dp(14), 0, dp(10))
        })

        savedTitle = sectionTitle("Meine PCs")
        column.addView(savedTitle)
        savedList = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(savedList)

        column.addView(sectionTitle("Im WLAN gefunden"))
        status = text("Suche nach TouchDeck …", 14f, Ui.MUTED)
        column.addView(LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(ProgressBar(context).apply { isIndeterminate = true; layoutParams = LinearLayout.LayoutParams(dp(22), dp(22)).apply { rightMargin = dp(10) } })
            addView(status)
            setPadding(0, 0, 0, dp(10))
        })
        foundList = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(foundList)

        column.addView(sectionTitle("Oder"))
        column.addView(button("📷  QR-Code vom PC scannen") { scanQr() }, fullWidth())
        column.addView(button("⌨️  Adresse von Hand eingeben") { manualDialog() }, fullWidth(top = 10))
        column.addView(text("Am PC: TouchDeck öffnen → „Handy verbinden“. Dort stehen QR-Code und Adresse.", 13f, Ui.MUTED).apply {
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
            val sub = "${server.host}:${server.port}" + if (server.isPaired) " · gekoppelt" else " · neu koppeln nötig"
            savedList.addView(card("🖥️  ${server.name}", sub, { connectSaved(server) }, { savedMenu(server) }), fullWidth(top = if (savedList.childCount > 0) 10 else 0))
        }
        if (saved.isNotEmpty()) savedList.addView(text("Lange drücken: umbenennen oder entfernen", 12f, Ui.MUTED).apply { setPadding(dp(4), dp(8), 0, 0) })
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
            .setItems(arrayOf("Umbenennen", "Neu koppeln", "Entfernen")) { _, which ->
                when (which) {
                    0 -> renameDialog(server)
                    1 -> pair(server)
                    2 -> AlertDialog.Builder(this)
                        .setMessage("„${server.name}“ aus der Liste entfernen?")
                        .setPositiveButton("Entfernen") { _, _ -> prefs.remove(server); renderSaved() }
                        .setNegativeButton("Abbrechen", null)
                        .show()
                }
            }
            .show()
    }

    private fun renameDialog(server: Server) {
        val input = EditText(this).apply { setText(server.name); setSelection(server.name.length) }
        AlertDialog.Builder(this)
            .setTitle("Name des PCs")
            .setView(LinearLayout(this).apply { setPadding(dp(22), dp(8), dp(22), 0); addView(input, fullWidth()) })
            .setPositiveButton("Speichern") { _, _ ->
                val name = input.text.toString().trim()
                if (name.isNotEmpty()) prefs.rename(server, name)
                renderSaved()
            }
            .setNegativeButton("Abbrechen", null)
            .show()
    }

    // ---------- Gefundene PCs ----------

    private fun addFound(server: Server) {
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
            foundList.addView(card("🖥️  ${server.name}", "$sub · antippen zum Koppeln", { pair(server) }), fullWidth(top = if (foundList.childCount > 0) 10 else 0))
        }
        status.text = when {
            fresh.isNotEmpty() -> if (fresh.size == 1) "1 neuer PC gefunden" else "${fresh.size} neue PCs gefunden"
            found.isNotEmpty() -> "Keine weiteren PCs – alle gefundenen sind schon gespeichert"
            else -> "Suche nach TouchDeck …"
        }
    }

    // ---------- Koppeln ----------

    private fun deviceName(): String =
        Settings.Global.getString(contentResolver, "device_name")?.takeIf { it.isNotBlank() } ?: "${Build.MANUFACTURER} ${Build.MODEL}"

    /** Kopplung anfragen und warten, bis am PC „Erlauben“ gedrückt wird. */
    private fun pair(server: Server) {
        val dialog = AlertDialog.Builder(this)
            .setTitle("Am PC bestätigen")
            .setMessage("Auf „${server.name}“ erscheint jetzt:\n„${deviceName()} möchte sich verbinden“.\n\nBitte dort auf „Erlauben“ klicken.")
            .setNegativeButton("Abbrechen") { _, _ -> pairing = null }
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
                        "denied" -> throw IllegalStateException("Am PC wurde die Verbindung abgelehnt.")
                        "expired" -> throw IllegalStateException("Die Anfrage ist abgelaufen. Bitte noch einmal versuchen.")
                    }
                }
                throw IllegalStateException("Keine Antwort vom PC. Ist das TouchDeck-Fenster am PC geöffnet?")
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
            "Der PC antwortet nicht. Läuft TouchDeck dort, und lässt die Firewall Verbindungen zu?"
        else -> e.message ?: "Verbindung fehlgeschlagen."
    }

    private fun manualDialog() {
        val last = prefs.servers.firstOrNull()
        val host = EditText(this).apply { hint = "IP-Adresse des PCs, z. B. 192.168.178.35"; inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI; setText(last?.host ?: "") }
        val port = EditText(this).apply { hint = "Port"; inputType = InputType.TYPE_CLASS_NUMBER; setText((last?.port ?: Prefs.DEFAULT_PORT).toString()) }
        AlertDialog.Builder(this)
            .setTitle("Adresse eingeben")
            .setView(LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(dp(22), dp(8), dp(22), 0)
                addView(host)
                addView(port)
            })
            .setPositiveButton("Verbinden") { _, _ ->
                val h = host.text.toString().trim().removePrefix("http://").substringBefore('/').substringBefore(':')
                val p = port.text.toString().toIntOrNull() ?: Prefs.DEFAULT_PORT
                if (h.isNotEmpty()) pair(prefs.servers.find { it.host == h && it.port == p } ?: Server(h, p))
            }
            .setNegativeButton("Abbrechen", null)
            .show()
    }

    // ---------- QR-Code ----------

    private fun scanQr() {
        IntentIntegrator(this)
            .setDesiredBarcodeFormats(IntentIntegrator.QR_CODE)
            .setPrompt("Den QR-Code aus „Handy verbinden“ am PC scannen")
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
            showMessage("Das ist kein TouchDeck-QR-Code. Am PC unter „Handy verbinden“ steht der richtige.")
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

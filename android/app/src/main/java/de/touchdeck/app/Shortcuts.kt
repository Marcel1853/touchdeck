package de.touchdeck.app

import android.app.Activity
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.nfc.NdefMessage
import android.nfc.NdefRecord
import android.nfc.NfcAdapter
import android.nfc.Tag
import android.nfc.tech.Ndef
import android.nfc.tech.NdefFormatable
import android.os.Build
import android.os.Bundle
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import android.widget.RemoteViews
import android.widget.Toast
import de.touchdeck.app.Ui.button
import de.touchdeck.app.Ui.dp
import de.touchdeck.app.Ui.text
import org.json.JSONArray

// Buttons am PC auslösen, ohne das Bedienfeld zu öffnen: Startbildschirm-Widget, Schnellzugriff-Kachel
// und Links/NFC-Aufkleber (touchdeck://press?server=…&page=…&button=…).

private fun store(context: Context) = context.getSharedPreferences("touchdeck_shortcuts", Context.MODE_PRIVATE)

/** Startbildschirm-Widget mit bis zu vier Buttons. */
class ButtonsWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) {
        for (id in ids) update(context, manager, id)
    }

    override fun onDeleted(context: Context, ids: IntArray) {
        store(context).edit().apply { ids.forEach { remove("widget_$it") } }.apply()
    }

    companion object {
        private val SLOTS = intArrayOf(R.id.slot0, R.id.slot1, R.id.slot2, R.id.slot3)

        fun buttons(context: Context, id: Int): List<RemoteButton> {
            val arr = JSONArray(store(context).getString("widget_$id", "[]"))
            return (0 until arr.length()).map { RemoteButton.fromJson(arr.getJSONObject(it)) }
        }

        fun update(context: Context, manager: AppWidgetManager, id: Int) {
            val list = buttons(context, id)
            val views = RemoteViews(context.packageName, R.layout.widget_buttons)
            SLOTS.forEachIndexed { i, slot ->
                val b = list.getOrNull(i)
                views.setViewVisibility(slot, if (b == null) View.GONE else View.VISIBLE)
                if (b != null) {
                    views.setTextViewText(slot, b.label)
                    val intent = Intent(context, PressReceiver::class.java).setAction(PressReceiver.ACTION).putExtra(PressReceiver.EXTRA, b.toJson().toString())
                    val pending = PendingIntent.getBroadcast(context, id * 10 + i, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
                    views.setOnClickPendingIntent(slot, pending)
                }
            }
            views.setViewVisibility(R.id.empty, if (list.isEmpty()) View.VISIBLE else View.GONE)
            manager.updateAppWidget(id, views)
        }
    }
}

/** Antippen eines Widget-Buttons: im Hintergrund am PC auslösen. */
class PressReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val json = intent.getStringExtra(EXTRA) ?: return
        val pending = goAsync()
        Remote.pressAsync(context, RemoteButton.fromJson(org.json.JSONObject(json))) { pending.finish() }
    }

    companion object {
        const val ACTION = "de.touchdeck.app.PRESS"
        const val EXTRA = "button"
    }
}

/** Einrichtung beim Anlegen des Widgets: Seite und bis zu vier Buttons wählen. */
class WidgetConfigActivity : Activity() {
    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(Lang.wrap(newBase))

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val id = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        setResult(RESULT_CANCELED, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id))
        if (id == AppWidgetManager.INVALID_APPWIDGET_ID) return finish()
        Remote.pick(this, 4, onCancel = { finish() }) { picked ->
            store(this).edit().putString("widget_$id", JSONArray(picked.map { it.toJson() }).toString()).apply()
            ButtonsWidget.update(this, AppWidgetManager.getInstance(this), id)
            setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id))
            finish()
        }
    }
}

/** Schnellzugriff-Kachel (Wischen von oben): ein gewählter Button. */
class ButtonTileService : TileService() {
    private fun chosen(): RemoteButton? = store(this).getString("tile", null)?.let { RemoteButton.fromJson(org.json.JSONObject(it)) }

    override fun onStartListening() {
        val tile = qsTile ?: return
        val b = chosen()
        val ctx = Lang.wrap(this)
        tile.label = b?.label ?: ctx.getString(R.string.tile_choose)
        tile.state = if (b == null) Tile.STATE_INACTIVE else Tile.STATE_ACTIVE
        tile.updateTile()
    }

    override fun onClick() {
        val b = chosen()
        if (b != null) {
            Remote.pressAsync(this, b)
            return
        }
        // Noch kein Button gewählt: Auswahl öffnen.
        val intent = Intent(this, TileConfigActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= 34) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}

/** Button für die Schnellzugriff-Kachel wählen (auch aus der App: „Schnellzugriff einrichten“). */
class TileConfigActivity : Activity() {
    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(Lang.wrap(newBase))

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Remote.pick(this, 1, onCancel = { finish() }) { picked ->
            store(this).edit().putString("tile", picked[0].toJson().toString()).apply()
            TileService.requestListeningState(this, ComponentName(this, ButtonTileService::class.java))
            Toast.makeText(this, getString(R.string.tile_set, picked[0].label), Toast.LENGTH_LONG).show()
            finish()
        }
    }
}

/**
 * Links und NFC-Aufkleber „touchdeck://press?…“: Button am PC auslösen und gleich wieder schließen.
 * Mit „touchdeck://write“ (aus der App) wird stattdessen ein Aufkleber beschrieben.
 */
class PressActivity : Activity() {
    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(Lang.wrap(newBase))

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val uri = intent?.data ?: NfcAdapter.getDefaultAdapter(this)?.let { readUri(intent) }
        val b = uri?.takeIf { it.scheme == "touchdeck" && it.host == "press" }?.let {
            val server = it.getQueryParameter("server")
            val page = it.getQueryParameter("page")
            val button = it.getQueryParameter("button")
            if (server.isNullOrEmpty() || page.isNullOrEmpty() || button.isNullOrEmpty()) null else RemoteButton(server, page, button, it.getQueryParameter("label").orEmpty())
        }
        if (b != null) Remote.pressAsync(this, b)
        finish()
    }

    private fun readUri(intent: Intent?): Uri? {
        @Suppress("DEPRECATION")
        val messages = intent?.getParcelableArrayExtra(NfcAdapter.EXTRA_NDEF_MESSAGES) ?: return null
        return messages.filterIsInstance<NdefMessage>().flatMap { it.records.toList() }.firstNotNullOfOrNull { it.toUri() }
    }
}

/** NFC-Aufkleber beschreiben: Button wählen, dann den Aufkleber an das Handy halten. */
class NfcWriteActivity : Activity(), NfcAdapter.ReaderCallback {
    private var link: String? = null
    private var label: String = ""
    private lateinit var info: android.widget.TextView

    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(Lang.wrap(newBase))

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val adapter = NfcAdapter.getDefaultAdapter(this)
        if (adapter == null) {
            Toast.makeText(this, R.string.nfc_missing, Toast.LENGTH_LONG).show()
            return finish()
        }
        info = text(getString(R.string.nfc_choose), 18f, bold = true).apply { gravity = Gravity.CENTER }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            setBackgroundColor(Ui.BG)
            setPadding(dp(28), dp(28), dp(28), dp(28))
            addView(text("📇", 64f).apply { gravity = Gravity.CENTER })
            addView(info, LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(16) })
            addView(button(getString(android.R.string.cancel)) { finish() }, LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(28) })
        })
        Remote.pick(this, 1, onCancel = { finish() }) { picked ->
            val b = picked[0]
            label = b.label
            link = b.copy(label = "").link()
            info.text = getString(R.string.nfc_hold, b.label)
            if (!adapter.isEnabled) info.text = getString(R.string.nfc_off)
        }
    }

    override fun onResume() {
        super.onResume()
        NfcAdapter.getDefaultAdapter(this)?.enableReaderMode(this, this, NfcAdapter.FLAG_READER_NFC_A or NfcAdapter.FLAG_READER_NFC_B or NfcAdapter.FLAG_READER_NFC_F or NfcAdapter.FLAG_READER_NFC_V, null)
    }

    override fun onPause() {
        NfcAdapter.getDefaultAdapter(this)?.disableReaderMode(this)
        super.onPause()
    }

    override fun onTagDiscovered(tag: Tag) {
        val target = link ?: return
        // URI-Eintrag plus App-Eintrag: Android öffnet direkt TouchDeck.
        val message = NdefMessage(arrayOf(NdefRecord.createUri(target), NdefRecord.createApplicationRecord(packageName)))
        val ok = try {
            val ndef = Ndef.get(tag)
            if (ndef != null) {
                ndef.connect()
                ndef.writeNdefMessage(message)
                ndef.close()
                true
            } else {
                NdefFormatable.get(tag)?.let { f ->
                    f.connect()
                    f.format(message)
                    f.close()
                    true
                } ?: false
            }
        } catch (_: Exception) {
            false
        }
        runOnUiThread {
            if (ok) {
                Toast.makeText(this, getString(R.string.nfc_written, label), Toast.LENGTH_LONG).show()
                finish()
            } else {
                info.text = getString(R.string.nfc_failed)
            }
        }
    }
}

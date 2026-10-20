package de.touchdeck.app

import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.widget.Toast
import org.json.JSONArray
import org.json.JSONObject
import java.net.URLEncoder
import kotlin.concurrent.thread

/** Ein Button am PC, wie ihn Widget, Schnellzugriff und NFC-Aufkleber kennen. */
data class RemoteButton(val serverKey: String, val page: String, val button: String, val label: String) {
    fun toJson(): JSONObject = JSONObject().put("server", serverKey).put("page", page).put("button", button).put("label", label)

    /** Link für NFC-Aufkleber und QR-Codes: touchdeck://press?server=…&page=…&button=… */
    fun link(): String {
        val e = { s: String -> URLEncoder.encode(s, "UTF-8") }
        return "touchdeck://press?server=${e(serverKey)}&page=${e(page)}&button=${e(button)}"
    }

    companion object {
        fun fromJson(o: JSONObject) = RemoteButton(o.getString("server"), o.getString("page"), o.getString("button"), o.optString("label"))
    }
}

/**
 * Buttons am PC auslösen, ohne das Bedienfeld zu öffnen (Widget, Schnellzugriff, NFC).
 * Alles läuft im Hintergrund; das Ergebnis kommt als kurze Meldung.
 */
object Remote {
    private val main = Handler(Looper.getMainLooper())

    private fun base(server: Server) = "http://${server.host}:${server.port}"
    private fun token(server: Server) = URLEncoder.encode(server.token, "UTF-8")

    /** Seiten mit Buttons (ID, Titel, Emoji) – klein, ohne den ganzen Zustand. */
    fun pages(server: Server): JSONArray {
        val conn = java.net.URL("${base(server)}/api/buttons?token=${token(server)}").openConnection() as java.net.HttpURLConnection
        try {
            conn.connectTimeout = 4000
            conn.readTimeout = 4000
            if (conn.responseCode != 200) throw Net.PcError(conn.responseCode)
            return JSONArray(conn.inputStream.bufferedReader().use { it.readText() })
        } finally {
            conn.disconnect()
        }
    }

    /** Löst den Button aus; liefert (geklappt, Meldung). */
    fun press(server: Server, page: String, button: String): Pair<Boolean, String> {
        val r = Net.call("POST", "${base(server)}/api/press?token=${token(server)}", JSONObject().put("pageId", page).put("buttonId", button), timeout = 6000)
        if (r.status == 401) return false to (r.body.optString("error").ifEmpty { "401" })
        val ok = r.status == 200 && r.body.optBoolean("ok")
        val messages = r.body.optJSONArray("messages")
        val text = if (ok) (0 until (messages?.length() ?: 0)).joinToString(" · ") { messages!!.getString(it) } else r.body.optString("error")
        return ok to text
    }

    /** Im Hintergrund auslösen und das Ergebnis als Toast zeigen. */
    fun pressAsync(context: Context, b: RemoteButton, done: (() -> Unit)? = null) {
        val ctx = Lang.wrap(context.applicationContext)
        val server = Prefs(ctx).servers.find { it.key == b.serverKey && it.isPaired }
        if (server == null) {
            toast(ctx, ctx.getString(R.string.remote_not_paired))
            done?.invoke()
            return
        }
        thread {
            val (ok, text) = try {
                press(server, b.page, b.button)
            } catch (_: Exception) {
                false to ctx.getString(R.string.remote_unreachable, server.name)
            }
            val label = b.label.ifEmpty { ctx.getString(R.string.remote_button) }
            toast(ctx, if (ok) "✓ $label${if (text.isNotEmpty()) " · $text" else ""}" else "✕ $label: $text")
            done?.let { main.post(it) }
        }
    }

    private fun toast(ctx: Context, text: String) = main.post { Toast.makeText(ctx, text, Toast.LENGTH_SHORT).show() }

    /**
     * Seite und Buttons am PC auswählen (gespeicherter, gekoppelter PC; bei mehreren erst den PC).
     * max = 1 für Schnellzugriff und NFC, mehr für das Widget.
     */
    fun pick(activity: Activity, max: Int, onCancel: () -> Unit, onDone: (List<RemoteButton>) -> Unit) {
        val servers = Prefs(activity).servers.filter { it.isPaired }
        when {
            servers.isEmpty() -> {
                AlertDialog.Builder(activity).setMessage(R.string.remote_not_paired).setPositiveButton(android.R.string.ok) { _, _ -> onCancel() }
                    .setOnCancelListener { onCancel() }.show()
            }
            servers.size == 1 -> pickPage(activity, servers[0], max, onCancel, onDone)
            else -> AlertDialog.Builder(activity).setTitle(R.string.remote_choose_pc)
                .setItems(servers.map { it.name }.toTypedArray()) { _, i -> pickPage(activity, servers[i], max, onCancel, onDone) }
                .setOnCancelListener { onCancel() }.show()
        }
    }

    private fun pickPage(activity: Activity, server: Server, max: Int, onCancel: () -> Unit, onDone: (List<RemoteButton>) -> Unit) {
        thread {
            val pages = try {
                pages(server)
            } catch (_: Exception) {
                null
            }
            activity.runOnUiThread {
                if (pages == null || pages.length() == 0) {
                    AlertDialog.Builder(activity).setMessage(activity.getString(R.string.remote_unreachable, server.name))
                        .setPositiveButton(android.R.string.ok) { _, _ -> onCancel() }.setOnCancelListener { onCancel() }.show()
                    return@runOnUiThread
                }
                val names = (0 until pages.length()).map { pages.getJSONObject(it).getString("name") }.toTypedArray()
                AlertDialog.Builder(activity).setTitle(R.string.remote_choose_page)
                    .setItems(names) { _, i -> pickButtons(activity, server, pages.getJSONObject(i), max, onCancel, onDone) }
                    .setOnCancelListener { onCancel() }.show()
            }
        }
    }

    private fun pickButtons(activity: Activity, server: Server, page: JSONObject, max: Int, onCancel: () -> Unit, onDone: (List<RemoteButton>) -> Unit) {
        val list = page.getJSONArray("buttons")
        val buttons = (0 until list.length()).map {
            val b = list.getJSONObject(it)
            val label = listOf(b.optString("emoji"), b.optString("title")).filter { s -> s.isNotEmpty() }.joinToString(" ").ifEmpty { b.getString("id") }
            RemoteButton(server.key, page.getString("id"), b.getString("id"), label)
        }
        val labels = buttons.map { it.label }.toTypedArray()
        val dialog = AlertDialog.Builder(activity).setOnCancelListener { onCancel() }
        if (max == 1) {
            dialog.setTitle(R.string.remote_choose_button).setItems(labels) { _, i -> onDone(listOf(buttons[i])) }.show()
            return
        }
        val chosen = BooleanArray(buttons.size)
        dialog.setTitle(activity.getString(R.string.remote_choose_buttons, max))
            .setMultiChoiceItems(labels, chosen) { d, i, on ->
                // Höchstens max Buttons: den zuletzt angehakten zurücknehmen.
                if (on && chosen.count { it } > max) {
                    chosen[i] = false
                    (d as AlertDialog).listView.setItemChecked(i, false)
                }
            }
            .setPositiveButton(android.R.string.ok) { _, _ ->
                val picked = buttons.filterIndexed { i, _ -> chosen[i] }
                if (picked.isEmpty()) onCancel() else onDone(picked)
            }
            .setNegativeButton(android.R.string.cancel) { _, _ -> onCancel() }
            .show()
    }
}

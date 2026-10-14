package de.touchdeck.app

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/** Ein gefundener oder gespeicherter PC. token ist leer, solange nicht gekoppelt. */
data class Server(
    val host: String,
    val port: Int,
    val name: String = host,
    val version: String = "",
    val token: String = "",
    val lastUsed: Long = 0,
) {
    val key: String get() = "$host:$port"
    val isPaired: Boolean get() = token.isNotEmpty()

    /** Adresse des Bedienfelds; der Schlüssel steht nur beim ersten Laden in der Adresse. */
    fun deckUrl(): String = "http://$host:$port/deck/?token=$token"

    fun toJson(): JSONObject = JSONObject()
        .put("host", host).put("port", port).put("name", name).put("token", token).put("lastUsed", lastUsed)

    companion object {
        fun fromJson(o: JSONObject) = Server(
            host = o.getString("host"),
            port = o.optInt("port", Prefs.DEFAULT_PORT),
            name = o.optString("name", o.getString("host")),
            token = o.optString("token"),
            lastUsed = o.optLong("lastUsed"),
        )
    }
}

/** Gespeicherte PCs (mehrere möglich) und welcher zuletzt benutzt wurde. */
class Prefs(context: Context) {
    private val prefs = context.getSharedPreferences("touchdeck", Context.MODE_PRIVATE)

    init {
        migrate()
    }

    /** Alle gespeicherten PCs, zuletzt benutzter zuerst. */
    val servers: List<Server>
        get() {
            val arr = JSONArray(prefs.getString("servers", "[]"))
            return (0 until arr.length()).map { Server.fromJson(arr.getJSONObject(it)) }.sortedByDescending { it.lastUsed }
        }

    /** Der PC, mit dem die App beim Start direkt verbindet. */
    val current: Server?
        get() = prefs.getString("current", null)?.let { key -> servers.find { it.key == key && it.isPaired } }

    private fun write(list: List<Server>) {
        prefs.edit().putString("servers", JSONArray(list.map { it.toJson() }).toString()).apply()
    }

    /** Speichert oder aktualisiert einen PC und macht ihn zum aktuellen. */
    fun save(server: Server) {
        val updated = server.copy(lastUsed = System.currentTimeMillis())
        write(servers.filter { it.key != server.key } + updated)
        prefs.edit().putString("current", updated.key).apply()
    }

    fun use(server: Server) = save(server)

    fun rename(server: Server, name: String) {
        write(servers.map { if (it.key == server.key) it.copy(name = name) else it })
    }

    fun remove(server: Server) {
        write(servers.filter { it.key != server.key })
        if (prefs.getString("current", null) == server.key) prefs.edit().remove("current").apply()
    }

    /** Schlüssel ungültig (z. B. am PC neu erzeugt): PC bleibt gespeichert, muss aber neu koppeln. */
    fun forgetToken(server: Server) {
        write(servers.map { if (it.key == server.key) it.copy(token = "") else it })
        prefs.edit().remove("current").apply()
    }

    /** Version 0.4.0 kannte nur einen PC – den in die Liste übernehmen. */
    private fun migrate() {
        val host = prefs.getString("host", null) ?: return
        val old = Server(host, prefs.getInt("port", DEFAULT_PORT), prefs.getString("name", host) ?: host, token = prefs.getString("token", "") ?: "")
        prefs.edit().remove("host").remove("port").remove("name").remove("token").apply()
        if (old.isPaired) save(old)
    }

    companion object {
        const val DEFAULT_PORT = 8770
    }
}

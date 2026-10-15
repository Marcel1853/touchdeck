package de.touchdeck.app

import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/** Kleine HTTP-Aufrufe an den TouchDeck-Server (Koppeln, Schlüssel prüfen). */
object Net {
    /** Der PC hat mit einem Fehlercode geantwortet, ohne eigene Meldung. */
    class PcError(val status: Int) : Exception()

    class Response(val status: Int, val body: JSONObject)

    fun call(method: String, url: String, body: JSONObject? = null, timeout: Int = 5000): Response {
        val conn = URL(url).openConnection() as HttpURLConnection
        try {
            conn.requestMethod = method
            conn.connectTimeout = timeout
            conn.readTimeout = timeout
            conn.setRequestProperty("X-TouchDeck", "1")
            if (body != null) {
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "application/json")
                conn.outputStream.use { it.write(body.toString().toByteArray()) }
            }
            val status = conn.responseCode
            val stream = if (status in 200..299) conn.inputStream else conn.errorStream
            val text = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
            val json = try { JSONObject(text) } catch (_: Exception) { JSONObject() }
            return Response(status, json)
        } finally {
            conn.disconnect()
        }
    }

    private fun base(server: Server) = "http://${server.host}:${server.port}"

    /** Fragt am PC an; dort erscheint „Gerät … möchte sich verbinden“. Liefert die Anfrage-ID. */
    fun requestPairing(server: Server, deviceName: String): String {
        val r = call("POST", "${base(server)}/api/pair/request", JSONObject().put("device", deviceName))
        if (r.status != 200) throw r.body.optString("error").takeIf { it.isNotEmpty() }?.let { IllegalStateException(it) } ?: PcError(r.status)
        return r.body.getString("id")
    }

    fun pairingStatus(server: Server, id: String): JSONObject =
        call("GET", "${base(server)}/api/pair/status?id=${URLEncoder.encode(id, "UTF-8")}").body

    /** Antwortet unter host:port ein TouchDeck? (Für die Suche im Netz, ohne Schlüssel.) */
    fun hello(host: String, port: Int): Server? = try {
        val r = call("GET", "http://$host:$port/api/hello", timeout = 700)
        if (r.status == 200 && r.body.optString("app") == "touchdeck") {
            Server(host, port, r.body.optString("name", host), r.body.optString("version"))
        } else null
    } catch (_: Exception) {
        null
    }

    /** 200 = Schlüssel gültig, 401 = ungültig (oder PIN fehlt), sonst nicht erreichbar. */
    fun checkToken(server: Server): Response =
        call("GET", "${base(server)}/api/state?token=${URLEncoder.encode(server.token, "UTF-8")}", timeout = 4000)
}

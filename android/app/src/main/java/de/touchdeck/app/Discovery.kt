package de.touchdeck.app

import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.NetworkInterface
import java.net.Inet4Address
import java.net.SocketTimeoutException
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/**
 * Sucht TouchDeck im WLAN: schickt regelmäßig eine UDP-Rundfrage, jeder PC mit TouchDeck antwortet
 * mit Name und Port. Manche Router lassen solche Rundfragen nicht durch – dann fragt die Suche
 * zusätzlich alle Adressen im eigenen Netz direkt per HTTP ab. Ein Schlüssel wird dabei nie übertragen.
 */
class Discovery(private val onFound: (Server) -> Unit) {
    @Volatile private var running = false
    private var socket: DatagramSocket? = null
    private val main = Handler(Looper.getMainLooper())

    fun start() {
        if (running) return
        running = true
        thread(name = "touchdeck-suche", isDaemon = true) { loop() }
        thread(name = "touchdeck-netzscan", isDaemon = true) {
            Thread.sleep(3000)
            if (running) scanSubnet()
        }
    }

    /** Eigene IPv4-Adresse im WLAN (z. B. 192.168.178.21). */
    private fun ownAddress(): Inet4Address? = try {
        NetworkInterface.getNetworkInterfaces().toList()
            .filter { it.isUp && !it.isLoopback }
            .sortedByDescending { it.name.startsWith("wlan") }
            .flatMap { it.inetAddresses.toList() }
            .filterIsInstance<Inet4Address>()
            .firstOrNull { it.isSiteLocalAddress }
    } catch (_: Exception) {
        null
    }

    /** Fragt alle 254 Adressen des eigenen /24-Netzes parallel nach /api/hello. */
    private fun scanSubnet() {
        val own = ownAddress() ?: return
        val prefix = own.hostAddress?.substringBeforeLast('.') ?: return
        val pool = Executors.newFixedThreadPool(32)
        for (i in 1..254) {
            val host = "$prefix.$i"
            if (host == own.hostAddress) continue
            pool.execute {
                if (!running) return@execute
                Net.hello(host, Prefs.DEFAULT_PORT)?.let { server -> main.post { if (running) onFound(server) } }
            }
        }
        pool.shutdown()
        pool.awaitTermination(30, TimeUnit.SECONDS)
    }

    fun stop() {
        running = false
        socket?.close()
    }

    private fun broadcastAddresses(): Set<InetAddress> {
        val result = mutableSetOf<InetAddress>(InetAddress.getByName("255.255.255.255"))
        try {
            for (nif in NetworkInterface.getNetworkInterfaces()) {
                if (!nif.isUp || nif.isLoopback) continue
                for (addr in nif.interfaceAddresses) addr.broadcast?.let { result += it }
            }
        } catch (_: Exception) {
            // Dann eben nur die allgemeine Broadcast-Adresse.
        }
        return result
    }

    private fun loop() {
        try {
            DatagramSocket().use { s ->
                socket = s
                s.broadcast = true
                s.soTimeout = 500
                val question = QUESTION.toByteArray()
                val buffer = ByteArray(1024)
                while (running) {
                    for (addr in broadcastAddresses()) {
                        try {
                            s.send(DatagramPacket(question, question.size, addr, PORT))
                        } catch (_: Exception) {
                            // Netz kurz weg – beim nächsten Durchlauf erneut.
                        }
                    }
                    val until = System.currentTimeMillis() + 2000
                    while (running && System.currentTimeMillis() < until) {
                        try {
                            val packet = DatagramPacket(buffer, buffer.size)
                            s.receive(packet)
                            val json = JSONObject(String(packet.data, 0, packet.length))
                            if (json.optString("app") != "touchdeck") continue
                            val server = Server(
                                host = packet.address.hostAddress ?: continue,
                                port = json.optInt("port", Prefs.DEFAULT_PORT),
                                name = json.optString("name", packet.address.hostAddress ?: ""),
                                version = json.optString("version"),
                            )
                            main.post { if (running) onFound(server) }
                        } catch (_: SocketTimeoutException) {
                            // weiter warten
                        } catch (_: Exception) {
                            // fremde oder kaputte Antwort ignorieren
                        }
                    }
                }
            }
        } catch (_: Exception) {
            // Socket wurde beim Beenden geschlossen.
        }
    }

    companion object {
        const val PORT = 8769
        private const val QUESTION = "TOUCHDECK_DISCOVER"
    }
}

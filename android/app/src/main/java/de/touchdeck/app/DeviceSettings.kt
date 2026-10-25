package de.touchdeck.app

import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.os.Build
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.Switch
import de.touchdeck.app.Ui.button
import de.touchdeck.app.Ui.dp
import de.touchdeck.app.Ui.immersive
import de.touchdeck.app.Ui.pressable
import de.touchdeck.app.Ui.text
import org.json.JSONObject

/**
 * Einstellungen nur für dieses Gerät. Leer bzw. -1 heißt „wie am PC eingestellt“; nur abweichende
 * Werte gehen an das Bedienfeld (window.TouchDeckApp.deviceSettings()).
 */
class DeviceSettings(context: Context) {
    private val p = context.getSharedPreferences("touchdeck_device", Context.MODE_PRIVATE)

    /** Ruhezustand nach Minuten: -1 = wie am PC, 0 = nie. */
    var sleepAfter: Int
        get() = p.getInt("sleepAfter", -1)
        set(v) = p.edit().putInt("sleepAfter", v).apply()

    /** "" = wie am PC, sonst dim, clock oder black. */
    var sleepMode: String
        get() = p.getString("sleepMode", "") ?: ""
        set(v) = p.edit().putString("sleepMode", v).apply()

    /** Helligkeit im Ruhezustand in Prozent (1–50). */
    var sleepBrightness: Int
        get() = p.getInt("sleepBrightness", 2)
        set(v) = p.edit().putInt("sleepBrightness", v).apply()

    /** Helligkeit im Betrieb: -1 = wie Android, sonst 5–100 %. */
    var brightness: Int
        get() = p.getInt("brightness", -1)
        set(v) = p.edit().putInt("brightness", v).apply()

    /** Bildschirm anlassen, solange das Bedienfeld offen ist. */
    var keepOn: Boolean
        get() = p.getBoolean("keepOn", true)
        set(v) = p.edit().putBoolean("keepOn", v).apply()

    /** Bewegen des Geräts zeigt auf dem Bildschirmschoner die Abkürzungen. */
    var motion: Boolean
        get() = p.getBoolean("motion", true)
        set(v) = p.edit().putBoolean("motion", v).apply()

    /** "" = wie am PC, "on" oder "off". */
    var vibrate: String
        get() = p.getString("vibrate", "") ?: ""
        set(v) = p.edit().putString("vibrate", v).apply()

    var sound: String
        get() = p.getString("sound", "") ?: ""
        set(v) = p.edit().putString("sound", v).apply()

    /** "" = wie am PC, sonst auto, portrait oder landscape. */
    var orientation: String
        get() = p.getString("orientation", "") ?: ""
        set(v) = p.edit().putString("orientation", v).apply()

    /** Nur die abweichenden Werte für das Bedienfeld. */
    fun toJson(): String = JSONObject().apply {
        if (sleepAfter >= 0) put("sleepAfter", sleepAfter)
        if (sleepMode.isNotEmpty()) put("sleepMode", sleepMode)
        if (vibrate.isNotEmpty()) put("vibrate", vibrate == "on")
        if (sound.isNotEmpty()) put("sound", sound == "on")
        if (orientation.isNotEmpty()) put("orientation", orientation)
    }.toString()

    companion object {
        /** Akzentfarbe von Android (ab Android 12, „Material You“) als #rrggbb; leer, wenn es keine gibt. */
        fun systemAccent(context: Context): String {
            if (Build.VERSION.SDK_INT < 31) return ""
            val c = context.getColor(android.R.color.system_accent1_200)
            return String.format("#%06x", c and 0xffffff)
        }
    }
}

/** Bildschirm „Einstellungen dieses Geräts“. */
class DeviceSettingsActivity : Activity() {
    private lateinit var s: DeviceSettings

    override fun attachBaseContext(newBase: Context) = super.attachBaseContext(Lang.wrap(newBase))

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        s = DeviceSettings(this)
        render()
    }

    private fun render() {
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(24), dp(28), dp(24), dp(28))
        }
        column.addView(text(getString(R.string.ds_title), 24f, bold = true))
        column.addView(text(getString(R.string.ds_hint), 14f, Ui.MUTED).apply { setPadding(0, dp(6), 0, dp(14)) })

        section(column, R.string.ds_sleep)
        val minutes = listOf(-1, 0, 1, 2, 5, 10, 15, 30, 60)
        choice(column, R.string.ds_sleep_after, minutes.map { minutesLabel(it) }, minutes.indexOf(s.sleepAfter).coerceAtLeast(0)) { s.sleepAfter = minutes[it] }
        val modes = listOf("", "dim", "clock", "black")
        choice(column, R.string.ds_sleep_mode, listOf(getString(R.string.ds_like_pc), getString(R.string.ds_mode_dim), getString(R.string.ds_mode_clock), getString(R.string.ds_mode_black)),
            modes.indexOf(s.sleepMode).coerceAtLeast(0)) { s.sleepMode = modes[it] }
        slider(column, R.string.ds_sleep_brightness, 1, 50, s.sleepBrightness) { s.sleepBrightness = it }
        toggle(column, R.string.ds_motion, s.motion) { s.motion = it }

        section(column, R.string.ds_screen)
        toggle(column, R.string.ds_keep_on, s.keepOn) { s.keepOn = it }
        toggle(column, R.string.ds_own_brightness, s.brightness >= 0) {
            s.brightness = if (it) 80 else -1
            render()
        }
        if (s.brightness >= 0) slider(column, R.string.ds_brightness, 5, 100, s.brightness) { s.brightness = it }
        val orientations = listOf("", "auto", "portrait", "landscape")
        choice(column, R.string.ds_orientation, listOf(getString(R.string.ds_like_pc), getString(R.string.ds_or_auto), getString(R.string.ds_or_portrait), getString(R.string.ds_or_landscape)),
            orientations.indexOf(s.orientation).coerceAtLeast(0)) { s.orientation = orientations[it] }

        section(column, R.string.ds_feedback)
        val onOff = listOf("", "on", "off")
        val onOffLabels = listOf(getString(R.string.ds_like_pc), getString(R.string.ds_on), getString(R.string.ds_off))
        choice(column, R.string.ds_vibrate, onOffLabels, onOff.indexOf(s.vibrate).coerceAtLeast(0)) { s.vibrate = onOff[it] }
        choice(column, R.string.ds_sound, onOffLabels, onOff.indexOf(s.sound).coerceAtLeast(0)) { s.sound = onOff[it] }

        column.addView(button(getString(R.string.ds_done), primary = true) { finish() },
            LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(24) })
        setContentView(ScrollView(this).apply {
            setBackgroundColor(Ui.BG)
            addView(column)
            // Abstand zu Kamera-Aussparung und (kurz eingeblendeten) Leisten.
            setOnApplyWindowInsetsListener { v, insets ->
                val bars = if (Build.VERSION.SDK_INT >= 30) {
                    insets.getInsets(android.view.WindowInsets.Type.systemBars() or android.view.WindowInsets.Type.displayCutout())
                } else null
                if (bars != null) v.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                insets
            }
        })
        immersive()
    }

    // Wie das Bedienfeld im Vollbild bleiben (auch nach Dialogen und dem Zurückkehren).
    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) immersive()
    }

    private fun minutesLabel(m: Int) = when (m) {
        -1 -> getString(R.string.ds_like_pc)
        0 -> getString(R.string.ds_never)
        else -> getString(R.string.ds_minutes, m)
    }

    private fun section(column: LinearLayout, title: Int) {
        column.addView(text(getString(title), 13f, Ui.ACCENT, bold = true).apply { setPadding(0, dp(18), 0, dp(6)) })
    }

    /** Zeile mit aktuellem Wert; Tippen öffnet die Auswahl. */
    private fun choice(column: LinearLayout, label: Int, options: List<String>, current: Int, onPick: (Int) -> Unit) {
        val value = text(options[current], 15f, Ui.MUTED)
        column.addView(row(getString(label), value) {
            AlertDialog.Builder(this).setTitle(label).setSingleChoiceItems(options.toTypedArray(), current) { d, i ->
                onPick(i)
                d.dismiss()
                render()
            }.show()
        })
    }

    private fun toggle(column: LinearLayout, label: Int, on: Boolean, onChange: (Boolean) -> Unit) {
        val sw = Switch(this).apply { isChecked = on; isClickable = false }
        column.addView(row(getString(label), sw) {
            sw.isChecked = !sw.isChecked
            onChange(sw.isChecked)
        })
    }

    private fun slider(column: LinearLayout, label: Int, min: Int, max: Int, value: Int, onChange: (Int) -> Unit) {
        val title = text("${getString(label)}: $value %", 15f)
        column.addView(title, LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(10) })
        column.addView(SeekBar(this).apply {
            this.max = max - min
            progress = value.coerceIn(min, max) - min
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(bar: SeekBar, p: Int, fromUser: Boolean) {
                    title.text = "${getString(label)}: ${p + min} %"
                    if (fromUser) onChange(p + min)
                }
                override fun onStartTrackingTouch(bar: SeekBar) {}
                override fun onStopTrackingTouch(bar: SeekBar) {}
            })
        })
    }

    private fun row(label: String, right: View, onClick: () -> Unit) = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        background = pressable(Ui.PANEL, 14, Ui.LINE)
        setPadding(dp(16), dp(14), dp(16), dp(14))
        layoutParams = LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(8) }
        addView(text(label, 15f), LinearLayout.LayoutParams(0, -2, 1f))
        addView(right)
        setOnClickListener { onClick() }
    }
}

package de.touchdeck.app

import android.app.Activity
import android.content.Context
import android.os.Build
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.content.res.ColorStateList
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.TextView

/** Kleine Helfer, damit die Oberflächen ohne XML-Layouts und ohne Zusatzbibliotheken auskommen. */
object Ui {
    val BG = Color.parseColor("#0D0F14")
    val PANEL = Color.parseColor("#171B24")
    val LINE = Color.parseColor("#262C3A")
    val TEXT = Color.parseColor("#E8EAF0")
    val MUTED = Color.parseColor("#8B93A7")
    val ACCENT = Color.parseColor("#7C8CFF")

    /** Vollbild: Status- und Navigationsleiste ausblenden; Wischen vom Rand zeigt sie kurz. */
    fun Activity.immersive() {
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

    fun Context.dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    fun Context.rounded(color: Int, radiusDp: Int = 16, stroke: Int? = null): GradientDrawable =
        GradientDrawable().apply {
            setColor(color)
            cornerRadius = dp(radiusDp).toFloat()
            if (stroke != null) setStroke(dp(1), stroke)
        }

    /** Hintergrund mit Druck-Effekt. */
    fun Context.pressable(color: Int, radiusDp: Int = 16, stroke: Int? = null) =
        RippleDrawable(ColorStateList.valueOf(Color.argb(60, 255, 255, 255)), rounded(color, radiusDp, stroke), null)

    fun Context.text(value: String, sizeSp: Float = 15f, color: Int = TEXT, bold: Boolean = false) = TextView(this).apply {
        text = value
        setTextColor(color)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, sizeSp)
        if (bold) typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
        setLineSpacing(0f, 1.15f)
    }

    fun Context.button(label: String, primary: Boolean = false, onClick: (View) -> Unit) = Button(this).apply {
        text = label
        isAllCaps = false
        setTextColor(if (primary) Color.WHITE else TEXT)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
        typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
        background = pressable(if (primary) ACCENT else PANEL, 14, if (primary) null else LINE)
        stateListAnimator = null
        minHeight = dp(52)
        setPadding(dp(18), 0, dp(18), 0)
        gravity = Gravity.CENTER
        setOnClickListener(onClick)
    }
}

package de.touchdeck.app

import android.content.Context
import android.content.res.Configuration
import java.util.Locale

/**
 * Sprache der App: die im TouchDeck am PC eingestellte (Kürzel wie am PC, z. B. „de“, „pt-br“, „zh-cn“).
 * Solange die App noch nie verbunden war, gilt die Sprache des Handys.
 */
object Lang {
    /** Dieselben Sprachen wie TouchDeck am PC; zu jeder gibt es res/values-<sprache>/strings.xml. */
    val SUPPORTED = setOf("de", "en", "es", "fr", "pl", "pt-br", "ru", "zh-cn")

    /** Context mit den Texten der gemerkten Sprache (für attachBaseContext und spätere Wechsel). */
    fun wrap(base: Context): Context {
        val lang = Prefs(base).language ?: return base
        val config = Configuration(base.resources.configuration)
        config.setLocale(Locale.forLanguageTag(lang))
        return base.createConfigurationContext(config)
    }
}

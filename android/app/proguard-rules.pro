# Methoden der JavaScript-Brücke werden aus dem Bedienfeld aufgerufen und dürfen nicht umbenannt werden.
-keepclassmembers class de.touchdeck.app.DeckActivity$Bridge {
    @android.webkit.JavascriptInterface <methods>;
}

# TouchDeck – Änderungsprotokoll

Alle wichtigen Änderungen an TouchDeck, der Makro-Fernbedienung für Linux, und an der Android-App.

[English](CHANGELOG.md) · **Deutsch**

```text
---------------------------------------------------------------------------------------------------
Version: 0.9.0
Datum: 2026-10-16
  Große Neuerungen:
    - Editor, Bedienfeld, Aktionskatalog, Server-Meldungen und Desktop-App gibt es vollständig auf
      Deutsch und Englisch. Alle Texte stehen in Übersetzungsdateien je Sprache; die Sprache lässt
      sich unter Einstellungen › Allgemein umschalten, offene Bedienfelder laden von selbst neu.
    - Freie Seiten: Neben dem Raster kann eine Seite „Frei“ sein. Buttons liegen mit beliebiger
      Lage und Größe auf einer Fläche mit wählbarem Format (16:9, 16:10, 4:3 …): ziehen zum
      Verschieben, an der Ecke vergrößern, Pfeiltasten schieben (mit Umschalt weiter). Der Wechsel
      zwischen Raster und frei behält alle Buttons.
    - Rasterseiten können eigene Spaltenbreiten und Zeilenhöhen haben.
    - Logik für Seiten: Jede Seite hat eigene Events, mit dem neuen Auslöser „Seite geöffnet /
      verlassen“ (auch beim Trennen eines Geräts) und dem Platzhalter {Event.Seite}.
    - Doppelt drücken: Buttons haben neben Drücken und Halten eine eigene Aktionsliste „Beim
      doppelten Drücken“. Nur Buttons mit Doppelt-Aktionen warten kurz (0,3 s) auf ein zweites
      Tippen, alle anderen reagieren sofort.
    - Profile lassen sich als .tdprofile-Datei exportieren und importieren (eine Seite oder alle
      Seiten, samt benutzten Vorlagen, Werten, Events und Bildern). Der Import fügt hinzu und
      überschreibt nichts.
    - Statusleiste unten im Editor: verbundene Geräte mit Name, Adresse und angezeigter Seite,
      Zustand von OBS, Twitch und Discord sowie Adresse und Version.
  Neuerungen:
    - Schrift-Optionen für Beschriftung und Live-Text: Schriftart (normal, Serif, Monospace,
      schmal, Handschrift – Schriften, die jedes Gerät hat, darum kann es leicht abweichen),
      fett, kursiv und Umriss (0–6 px) für Text auf Bildern. „Aussehen kopieren“ überträgt sie.
    - Live-Anzeige „Seitenanzeige (●○○)“ zeigt die Position der Seite in der Seitenliste, und
      „Gehe zu Seite“ kann jetzt zur nächsten / vorherigen Seite in der Liste springen.
    - Timer: Aktion „Timer (Countdown / Stoppuhr)“ mit Starten, Anhalten und Zurücksetzen,
      Live-Anzeige „Timer“ mit sekundengenauer Restzeit und Auslöser „Timer abgelaufen“.
    - Neuer Auslöser „Programm im Vordergrund“ und Aktion „Seite auf allen Geräten anzeigen“:
      z. B. wechseln Handy und Tablet von selbst auf die OBS-Seite, sobald OBS im Vordergrund ist.
    - Neue Aktion „Zufällig: eine der nächsten Aktionen“: führt genau eine zufällig gewählte der
      folgenden Aktionen aus (Anzahl einstellbar, 0 = alle folgenden) und überspringt die anderen.
    - Rechtsklick auf ein leeres Feld oder die freie Fläche › „Ordner hier anlegen“: legt eine
      Unterseite mit Zurück-Button an und an dieser Stelle einen Button, der sie öffnet.
    - Rechtsklick › „Logik kopieren“ / „Logik einfügen“ überträgt alle Aktionen (Drücken, Aus,
      Halten, Loslassen, Doppelt) und Events eines Buttons auf einen anderen.
    - Buttons können für den Zustand „an“ ein eigenes Symbol und eine eigene Beschriftung haben
      (z. B. „Ton an“ / „Stumm“). „An“ ist ein eingeschalteter Umschalter oder eine Live-Anzeige,
      die „an“ meldet; die Farbe für „an“ gibt es jetzt auch bei Live-Anzeigen.
    - Neuer Event-Auslöser „Ton oder Mikrofon stumm/laut geschaltet“; der Stumm-Button der
      Startseite nutzt ihn als Beispiel.
    - Fertige Seiten (System, OBS, Steam) mit sichtbarer Logik neu aufgebaut: Buttons zeigen ihren
      Zustand über Events mit „Aussehen ändern“ und „Ursprüngliches Aussehen wiederherstellen“
      statt über versteckte Live-Logik. OBS-Szenen-Buttons vergleichen mit dem neuen Wert
      „OBS-Szene“.
    - Steam-Seiten halten sich selbst aktuell: Sie werden beim Start und beim Öffnen einer
      Steam-Seite geprüft, vorhandene Spiel-Buttons behalten Reihenfolge und eigene Änderungen,
      deinstallierte Spiele fallen weg, neue kommen hinten dazu.
    - Fehlende Spiel-Cover werden einmal von Steams Bildserver geladen.
    - Neue Bedienfeld-Einstellung „Form der Felder“: „Fläche füllen“ lässt Felder etwas breiter
      oder höher als quadratisch werden, damit Seiten den Bildschirm besser nutzen; „Quadratisch“
      bleibt wählbar. Abstände und Rand passen sich der Größe an.
    - Icon-Pakete haben einen eigenen Bereich in der linken Leiste; Strg+1 … 8 wechselt die
      Bereiche.
  Kleine Neuerungen:
    - Platzhalter verstehen auch englische Namen, z. B. {Time}, {Date} und {Event.User}; der
      Editor zeigt Hinweise für alle Auslöser.
    - Auslöser „Umschalter wird an/aus geschaltet“ zeigt einen Hinweis, wenn es noch keinen Button
      mit Verhalten „Umschalter“ gibt.
    - Auslöser ohne „vorherige Seite“ bieten eine Seitenauswahl.
    - Abzeichen „2×“ im Editor für Buttons mit Doppelt-Aktionen.
    - Das Bedienfeld meldet nach dem Verbinden seinen Namen; die Android-App liefert den
      Gerätenamen aus ihren Einstellungen, Browser ihre Kennung.
    - /api/info liefert die Liste der verbundenen Geräte.
  Änderungen:
    - Ein einfacher Klick auf ein leeres Rasterfeld oder die freie Fläche legt keinen Button mehr
      an, sondern hebt nur die Auswahl auf. Neue Buttons entstehen per Doppelklick, Rechtsklick
      oder „＋ Button“ in der Werkzeugleiste.
    - Live-Anzeigen nur noch für Zahlen (Uhrzeit, CPU, RAM, Temperatur, Musiktitel,
      Steam-Updates).
    - Freie Seiten werden auf Handy und Tablet genau im gewählten Format gezeigt, wie in der
      Editor-Vorschau, statt gedehnt zu werden.
    - Werkzeugleiste im Editor kompakter.
    - READMEs und Plugin-Anleitung sind jetzt auf Englisch, mit deutschen Fassungen und
      Sprachumschalter oben; die Funktionslisten sind auf dem aktuellen Stand.
  Fehlerbehebungen:
    - Der Autostart-Eintrag zeigte nach einem Update auf eine nicht mehr vorhandene Datei; er
      folgt jetzt bei jedem Start der laufenden Version.
    - Fehlendes TouchDeck-Symbol in Taskleiste und Startmenü behoben; dafür wird jetzt ein
      Starter-Eintrag mit Symbol im Benutzerordner angelegt.
  Übersetzung:
    - Menü der Systemleiste, Hinweise und Startmeldungen der Desktop-App folgen der eingestellten
      Sprache.
    - Fehler, Live-Texte, Logs, Startmeldung und Datum des Servers folgen der eingestellten
      Sprache.
    - Den Bestätigungssatz für den erweiterten Modus gibt es auch auf Englisch.
    - Tests prüfen, dass beide Sprachen dieselben Schlüssel und Platzhalter haben.
  Android-App:
    - App 0.6.0: Die App gibt es auf Deutsch und Englisch.
    - App 0.6.1: Die Sprache der App folgt der TouchDeck-Einstellung, auch auf dem
      Verbinden-Bildschirm (vor der ersten Verbindung: Sprache des gefundenen PCs, sonst die des
      Handys).
    - App 0.6.2: Die App liefert den Gerätenamen aus ihren Einstellungen für die Statusleiste
      im Editor.

---------------------------------------------------------------------------------------------------
Version: 0.8.0
Datum: 2026-10-14
  Neuerungen:
    - Ausrichtung für Handy und Tablet einstellbar: automatisch, Hochformat oder Querformat. Die
      Android-App dreht sich sofort (automatisch folgt der Drehsperre des Geräts), im Browser gilt
      die Einstellung im Vollbild.
  Änderungen:
    - Editor und Bedienfeld gehören jetzt zur Rust-Fassung; die alte Electron-Fassung wurde
      entfernt.
    - Die README der Rust-Fassung ist jetzt die Hauptanleitung (Handy verbinden, Aktionen,
      Twitch, Discord, OBS).
    - Neue Einstellung „Sprache“ (Deutsch/Englisch); Standard ist die Systemsprache.
  Plugins:
    - Erweiterte Plugins mit vollen Rechten: eigene Programme (Python, Node, …), die TouchDeck
      startet und über JSON-Zeilen auf stdin/stdout anspricht, ohne offenen Port.
    - Erweiterte Plugins laufen nur im erweiterten Modus, der mit doppelter Bestätigung (Warnung
      und Eintippen von „AUF EIGENE GEFAHR“) eingeschaltet wird; jedes Plugin muss beim
      Installieren zusätzlich bestätigt werden. Ist der Modus aus, sind sie gesperrt und im
      Editor ausgeblendet.
    - Auch erweiterte Plugins nur aus öffentlichen GitHub-Repos oder im Testmodus. Die Prüfsumme
      der Vorschau deckt jetzt alle Dateien ab.
    - Abgestürzte erweiterte Plugins werden höchstens dreimal pro Minute neu gestartet.
    - plugin.json und Marktplatz-Einträge dürfen eine Lizenz angeben; der Editor zeigt sie in
      Vorschau, Plugin-Karte und Marktplatz.
    - Texte in plugin.json dürfen Übersetzungen sein ({ "en": …, "de": … }); TouchDeck zeigt die
      eingestellte Sprache, sonst Englisch.
    - Beispiel-Plugins liegen jetzt im öffentlichen Repo touchdeck-plugins; das Plugin-Wiki ist im
      Editor verlinkt.
    - Installation per GitHub-Link ohne Marktplatz-Eintrag zeigt einen Hinweis „auf eigene
      Gefahr“.
  Android-App:
    - App 0.5.0: folgt der Einstellung für die Ausrichtung.

---------------------------------------------------------------------------------------------------
Version: 0.7.0
Datum: 2026-10-14
  Große Neuerungen:
    - Icon-Pakete: eigene Bildersammlungen anlegen, Bilder hinzufügen und entfernen, als .tdicons
      exportieren und importieren; Auswahl mit Suche beim Button.
  Plugins:
    - Plugin-System: Erweiterungen in JavaScript, abgeschottet in QuickJS mit Speicher- und
      Zeitgrenze. Zugriff nur über td.* und nur mit den in plugin.json angegebenen Berechtigungen
      (Netz nur zu genannten Adressen ohne Weiterleitungen, Werte, Benachrichtigungen).
    - Plugins werden nur aus öffentlichen GitHub-Repos installiert, mit Vorschau von Quelle,
      Version und Berechtigungen; installiert wird genau der geprüfte Stand.
    - Marktplatz-Tab, Testmodus für lokale Ordner, Beispiel-Plugin und Plugin-Anleitung
      (PLUGINS.md).

---------------------------------------------------------------------------------------------------
Version: 0.6.0
Datum: 2026-10-14
  Große Neuerungen:
    - Erste Veröffentlichung: Makro-Fernbedienung für Linux mit Editor, Bedienfeld fürs Handy,
      Werten, Events, fertigen Seiten, Logs und Sicherungen.
    - Anbindung an Steam, OBS und Twitch.
    - Discord-Anbindung über das lokale Discord-Programm: stumm, taub, Sprachkanal betreten/
      verlassen, Lautstärke, Push-to-Talk; Events und Live-Anzeigen für stumm, taub und
      Sprachkanal; Nachrichten in einen Kanal per Webhook.
    - Import von Seiten aus einem anderen Makro-Programm.
    - Android-App mit automatischer Suche im WLAN, Kopplung per Bestätigung oder QR-Code und
      mehreren gespeicherten PCs.
    - TouchDeck neu in Rust mit Tauri: Server, alle Aktionen, Events, Live-Werte, Regler,
      Gedrückthalten, Sicherungen, Koppeln und Suche im WLAN. Pakete als .deb und AppImage; die
      Daten bleiben dieselben.
  Neuerungen:
    - Neuer Aktions-Editor als großes Fenster: Kategorien und Suche links, Aktionen als Kacheln in
      der Mitte, Ablauf rechts; hinzufügen per Klick oder Ziehen, sortieren per Ziehen, „Zuletzt
      benutzt“.
    - Die Eigenschaften-Spalte zeigt Aktionen kompakt; ein Klick öffnet den Aktions-Editor.
    - Rechtsklick-Menü für Buttons: testen, duplizieren, kopieren, ausschneiden, einfügen,
      Aussehen übertragen, auf andere Seiten verschieben oder kopieren; Strg+C/X/V.
  Kleine Neuerungen:
    - Spaltenbreiten im Editor per Ziehgriff verstellbar und gemerkt (Doppelklick setzt zurück).
    - „Fertige Seiten“ in der Seitenleiste einklappbar.
    - Die Informationsseite zeigt die Laufzeit an.
  Optimierungen:
    - Tastatur, Maus und Fenster laufen direkt über X11/XTest statt über Python.
```

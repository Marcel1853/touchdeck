# TouchDeck – Änderungsprotokoll

Alle wichtigen Änderungen an TouchDeck, der Makro-Fernbedienung für Linux, und an der Android-App.

[English](CHANGELOG.md) · **Deutsch**

```text
---------------------------------------------------------------------------------------------------
Version: 0.12.0
Datum: 2026-10-25
  Neuerungen:
    - KI-Assistenten (MCP): Einstellungen › KI-Assistenten schaltet einen MCP-Server ein. Assistenten
      wie Claude sehen Seiten, Buttons und Live-Werte, drücken Buttons, lesen und setzen Werte und
      wechseln die Seite auf den Geräten. Mit zweitem Haken dürfen sie auch Seiten und Buttons anlegen,
      ändern und löschen und beliebige Aktionen ausführen. Nur vom PC aus, mit eigenem Schlüssel,
      jede Nutzung im Protokoll; fertige Befehle für Claude Code und andere Programme zum Kopieren.
    - Plugins dürfen den Bildschirmschoner füllen: td.saver.set({ title, items, color }) zeigt eine Karte
      unter der Uhr (z. B. Forschung und Strom aus Factorio); abschaltbar in den Einstellungen.
    - Bedienfeld: Seitenauswahl oben links als deutlicher Auswahlknopf; in der App oben rechts
      „Verbindung ändern“ (⇄) und „Einstellungen dieses Geräts“ (⚙).

---------------------------------------------------------------------------------------------------
Version: 0.11.0
Datum: 2026-10-20
  Große Neuerungen:
    - Buttons mit mehreren Zuständen: Button-Art „Zustände“ – jeder Druck schaltet zum nächsten
      Zustand mit eigenem Titel, Emoji, Farben und eigenen Aktionen. Wahlweise „Zustand nach Wert“:
      Der Zustand richtet sich nach einem Wert (z. B. „10..50“, „>80“, „an“).
    - Schalter als neue Button-Art: Kippschalter waagerecht oder senkrecht, an/aus mit eigenen
      Aktionen wie ein Umschalter.
    - Aktionen mit Rückgabe als Daten (Plugins): Eine Plugin-Aktion kann Daten zurückgeben
      (z. B. { players: 7, memory: 4210, uptime: 50432 }). TouchDeck zeigt sie automatisch als Liste
      mit lesbaren Einheiten („RAM 4,1 GB“, „Laufzeit 14 h“) und reicht sie als {Ergebnis.players}
      an folgende Aktionen weiter. Neu im Manifest: „returns“.
    - Android-App 0.7.0/0.8.0: Startbildschirm-Widget mit bis zu vier Buttons, Schnellzugriff-Kachel und
      NFC-Aufkleber lösen Buttons am PC aus, ohne das Bedienfeld zu öffnen.
  Neuerungen:
    - Sicherheitsabfrage: Buttons mit „Erst beim zweiten Druck auslösen“ zeigen beim ersten Druck
      „Sicher?“.
    - Abhängigkeiten: Ein Button löst nur aus, wenn ein anderer Button an ist oder ein Wert passt
      (z. B. erst „Scharf schalten“, dann „Senden“); sonst ist er ausgegraut mit Schloss.
    - Versteckte Buttons: „Anfangs versteckt“ und die Aktion „Button zeigen / verstecken“ (auch nur
      für einige Sekunden).
    - Auslöser „Programm im Vordergrund“: Haken „Nur beim ersten Mal“ – löst erst wieder aus, wenn das
      Programm beendet und neu gestartet wurde (z. B. Seite fürs Spiel nur beim Start zeigen).
    - Aktion „Zustand eines Buttons setzen“ (nächster, vorheriger, Nr.).
    - „Benachrichtigung zeigen“: wahlweise am PC, im Bedienfeld oder beides. Im Bedienfeld erscheint
      sie auf dem Gerät, auf dem gedrückt wurde (bei Events auf allen).
    - Rückmeldung am Button: kurzes Häkchen, wenn die Aktionen geklappt haben (abschaltbar).
    - Ruhezustand fürs Bedienfeld: nach einigen Minuten abdunkeln, Uhr oder schwarz zeigen; der
      erste Tipp weckt nur. In der App wird dabei auch die Helligkeit abgesenkt.
    - Bildschirmschoner: große Uhr auf Schwarz (digital dünn, kräftig oder analog) mit Datum, eigene
      Farbe oder Systemfarbe von Android; sie wandert jede Minute ein Stück gegen Einbrennen. Bis zu vier
      Abkürzungen erscheinen beim ersten Tipp (in der App auch beim Bewegen des Geräts).
    - PC gesperrt: Bedienfeld sperren, solange der PC gesperrt ist, und Auslöser „PC gesperrt /
      entsperrt“ (Cinnamon, GNOME, KDE, MATE).
    - USB-Verbindung: Einstellungen › Handy & Sicherheit › „Android-Geräte per USB-Kabel verbinden“
      leitet den Port an angeschlossene Geräte mit USB-Debugging weiter („adb reverse“, braucht adb
      am PC); in der App „Per USB-Kabel verbinden“ wählen. Kein WLAN nötig.
    - Marktplatz: Knopf „Marktplatz neu laden“ holt die Liste frisch von GitHub, neue Plugins
      erscheinen sofort (kein Neustart nötig).
    - Menü am Symbol in der Systemleiste: „TouchDeck neu starten“.
  Fehlerbehebungen:
    - „Tastenkürzel drücken“ an ein bestimmtes Programm: sucht nur noch nach dem Programm, nicht nach
      dem Fenstertitel, und drückt nur, wenn genau dieses Fenster vorne ist. Läuft das Programm nicht,
      kommt eine Meldung statt Tasten im falschen Fenster (z. B. in einem Editor mit „Factorio“ im Titel).
  Leistung:
    - Werte, Umschalter und Aussehen werden gesammelt gespeichert statt bei jeder Änderung die ganze
      Konfiguration zu schreiben; config.json ist kompakt.
    - Geräte bekommen nur noch, was sich geändert hat: kleine Nachrichten statt des ganzen Zustands,
      Live-Werte nur für die eigene Seite und nur geänderte, Graphen nur neue Punkte. Das Tablet
      zeichnet nur geänderte Buttons neu.
    - Plugin-, Timer- und Wert-Anzeigen werden sofort und ohne Abfrage aller anderen Werte
      aktualisiert.
    - „Programm im Vordergrund“ über eine stehende X11-Verbindung, Tastenkürzel und Zwischenablage
      warten auf Ereignisse statt bis zu 50-mal pro Sekunde nachzusehen.
    - Ton-Auslöser über „pactl subscribe“ statt alle 2 Sekunden nachzufragen; Steam-Erkennung liest
      nur neue Prozesse.
    - Bedienfeld im Hintergrund (App verlassen, Bildschirm aus) bekommt keine Live-Werte mehr; die
      App pausiert dabei die Seite.
  Android-App:
    - App 0.6.4: neuer Knopf „Per USB-Kabel verbinden“.
    - App 0.8.0: „Einstellungen dieses Geräts“ (Verbinden-Bildschirm und Seitenmenü im Bedienfeld):
      Ruhezustand und Art je Gerät, Helligkeit im Ruhezustand und im Betrieb, Bildschirm anlassen,
      Ausrichtung, Vibration und Klick-Ton; Bewegen zeigt die Abkürzungen des Bildschirmschoners.
    - App 0.8.1: Einstellungen im Vollbild wie das Bedienfeld; Zahnrad oben rechts im Bedienfeld.
    - App 0.7.0: Widget, Schnellzugriff-Kachel, NFC-Aufkleber beschreiben (Verbinden-Bildschirm ›
      Abkürzungen), Links „touchdeck://press?…“; Ruhezustand senkt die Helligkeit.

---------------------------------------------------------------------------------------------------
Version: 0.10.0
Datum: 2026-10-18
  Neuerungen:
    - Designs aus Plugins und Icon-Paketen: Aussehen › Designs zeigt sie als Vorschau, ein Klick
      übernimmt Farben, Schrift, Textur, Overlay und Regler-Form. Plugins können Icon-Pakete
      voraussetzen, die beim Installieren importiert werden.
    - Port und Adresse (IP oder Name für Links und QR-Codes) lassen sich unter Einstellungen ›
      Handy & Sicherheit einstellen.
    - Events bekommen Ordner, Mehrfachauswahl und ein Rechtsklick-Menü; Plugin-Karten lassen sich
      einklappen; Plugins und Icon-Pakete haben ein Rechtsklick-Menü.
    - Neue Regler-Stile „Drehknopf (wie am Mischpult)“ mit Skala, Rillenrand und Zeiger sowie „Fader +
      Drehknopf“: ein Button mit Fader für ein Ziel und Drehknopf für ein zweites (z. B. Lautstärke
      und Mikrofon).
    - Fertige Seiten eines Plugins dürfen auch in Unterordnern des Plugins liegen (z. B.
      pages/strom/…); Seiten-Ordner bleiben beim Export und Import von Profilen erhalten.
    - Ordner für Seiten und Vorlagen: eigene Ordner anlegen (auch leere), einklappen, Seiten und
      Vorlagen per Ziehen oder Rechtsklick › „In Ordner verschieben“ hineinlegen; Rechtsklick auf einen
      Ordner benennt ihn um, löst ihn auf (Inhalt bleibt) oder löscht ihn samt Inhalt.
    - Mehrere Seiten oder Vorlagen auf einmal: Strg+Klick oder Shift+Klick wählt mehrere aus,
      Rechtsklick löscht, verschiebt oder (Seiten) exportiert sie zusammen.
    - Werte: Rechtsklick auf einen Wert (umbenennen, in Ordner verschieben, löschen) und auf einen
      Ordner (umbenennen, auflösen, samt Werten löschen).
    - Rechtsklick auf eine Seite in der Seitenliste: öffnen, umbenennen, duplizieren, als Startseite,
      nach oben/unten und löschen, neben Export und Import.
    - Graphen mit mehreren Datenreihen: weitere Quellen hinzufügen (je mit Name und Farbe) oder von
      einem Plugin mit „series“ liefern lassen – neue Namen werden von selbst zu neuen Linien. Neue
      Darstellung „Fläche gestapelt“; Balken mit mehreren Reihen werden gestapelt; eine Legende
      zeigt die Namen.
    - Neue Button-Art „Anzeige“: reagiert nicht auf Antippen (keine Aktionen, keine
      Drück-Animation) – für Live-Werte und Graphen. Mit „Transparent“ bleibt nur der Inhalt; auf
      freien Seiten beliebig groß.
    - Graphen: jede Live-Anzeige mit einer Zahl kann ihren Verlauf als Linie, Fläche oder Balken
      zeigen (Anzahl Werte, Abstand, feste oder automatische Grenzen, Farbe). Der Verlauf wird im
      Hintergrund auch aufgezeichnet, wenn die Seite nicht offen ist; größere Buttons ergeben größere
      Diagramme. Plugins können mit „number“ echte Zahlen melden (z. B. 35,2 MW). Wert und Titel
      stehen unter dem Graphen, links, mittig oder rechts ausgerichtet.
    - Marktplatz: „📖 Beschreibung“ zeigt die README eines Plugins (auf Deutsch die README.de.md) mit
      Überschriften, Listen, Tabellen und Bildern, mit Installieren-Knopf direkt dabei.
    - Plugins dürfen in einem Unterordner eines Repos liegen (mehrere Plugins in einem Repo):
      Marktplatz-Einträge kennen „path“, und „besitzer/repo/ordner“ oder ein kopierter
      GitHub-Ordner-Link lassen sich direkt eingeben.
    - Plugins können fertige Seiten mitbringen: .tdprofile-Dateien im Plugin-Ordner erscheinen in der
      Plugin-Karte und unter „Fertige Seiten“ und werden mit einem Klick importiert.
    - Neue Button-Art „Webseite“: zeigt eine Webseite direkt im Button (Adresse, Zoom, auf dem Gerät
      bedienbar, alle n Sekunden neu laden) – z. B. ein Dashboard oder eine Status-Seite im Heimnetz.
    - Neuer Regler-Stil „Fader (wie am Mischpult)“: dunkle Schiene, farbige Füllung und eckiger
      Griff. Regler mit Schiene nehmen den Wert jetzt entlang der Schiene.
    - Werte werden nach Ordnern in einklappbaren Gruppen gezeigt, mit „Neuer Ordner“ und einem „＋“
      je Ordner.
    - Symbolgröße je Button (30–200 %), Textur anpassen (füllen, ganz zeigen, strecken, kacheln) und
      ein einstellbarer Farbschleier über der Textur (0 % = Textur unverändert); „Aussehen
      übertragen“ nimmt sie mit.
    - „Aussehen ändern“ kann auch ein Bild als Symbol, eine Textur und ein Overlay setzen, z. B. aus
      einem Event; die Bilder werden mit Vorschau aus eigenen Bildern und Icon-Paketen gewählt.
    - Neue Live-Anzeigen: Temperatur und Auslastung der Grafikkarte, belegte Festplatte,
      Netzwerk-Tempo (↓/↑) und wie lange der PC schon läuft.
    - Neue Aktion „Bildschirm (aus / Helligkeit)“: schaltet den Bildschirm sofort aus (die Maus weckt
      ihn) oder setzt die Helligkeit – echt über ddcutil, falls installiert, sonst abgedunkelt.
    - „Tastenkürzel-Seite erstellen …“ (Seitenmenü „⋯“): eine Liste wie „Rückgängig = ctrl+z | ↩️“
      wird zu einer Seite, deren Buttons die Kürzel an ein gewähltes Programm schicken; Listen lassen
      sich aus Textdateien laden.
    - „Tastenkürzel drücken“ kann das Kürzel an ein gewähltes Programm schicken: es wird vorher nach
      vorn geholt, danach geht TouchDeck auf Wunsch zum vorigen Fenster zurück.
    - Neue Aktionen „In die Zwischenablage kopieren“ (mit Platzhaltern, auf Wunsch gleich einfügen
      – auch für Text mit Zeichen, die das Tastaturlayout nicht hat) und „Zwischenablage in Wert
      speichern“.
    - „Uhrzeit erreicht“ kennt mehrere Uhrzeiten (08:00, 18:30), einzelne Wochentage und einen Tag
      im Monat.
    - Timer-Aktion „Wecker: bis Uhrzeit herunterzählen“ (z. B. 07:30, heute oder morgen); „Timer
      abgelaufen“ löst aus, wenn sie erreicht ist.
    - Neuer Auslöser „Tastenkürzel am PC gedrückt“: eine Tastenkombination irgendwo am PC (X11)
      startet Events, auch wenn TouchDeck im Hintergrund ist; von anderen Programmen belegte Kürzel
      stehen im Protokoll.
    - Neue Aktion „Tastenfolge (aufgenommen)“: Tasten mit ihren echten Pausen im Editor aufnehmen,
      als Liste bearbeiten (z. B. ctrl+c, 250ms, ctrl+v) und mit einstellbarem Tempo abspielen.
    - Neue Aktion „Wert berechnen“: + − × ÷ %, Potenzen, Klammern und round/floor/ceil/abs/sqrt/
      min/max mit Platzhaltern, z. B. ({Tore} + 1) * 2; Kommazahlen aus Werten werden verstanden.
    - „HTTP-Anfrage“ kann die Antwort – oder ein Feld einer JSON-Antwort wie data.0.name – in einem
      Wert speichern; Adresse, Header und Inhalt verstehen Platzhalter.
    - „Befehl ausführen“ kann die Ausgabe in einem Wert speichern.
    - Vorlagen mit Eingaben: eine Vorlage legt Eingaben mit Standardwert fest (z. B. „Szene“),
      „Vorlage ausführen“ füllt sie je Aufruf (Platzhalter erlaubt), im Ablauf stehen sie als
      {Eingabe.Name} bereit.
    - Logik-Blöcke für Aktionslisten: „Wenn …“ / „Sonst wenn …“ / „Sonst“, „Wiederholen (n-mal)“,
      „Solange …“ und „Schleife verlassen“, jeweils bis „Ende“. Der Editor rückt die Blöcke ein und
      setzt das „Ende“ selbst dazu. Der aktuelle Durchlauf steht als {Event.Durchlauf} bereit;
      Endlosschleifen stoppen nach 20000 Schritten.
    - Bedingungen können Platzhalter vergleichen (z. B. {Event.Nutzer}) und kennen „größer oder
      gleich“, „kleiner oder gleich“ und „ist nicht leer“.
  Fehlerbehebungen:
    - Liegende Fader sitzen mittig im Button.
    - Offene Bedienfelder (Handy, Tablet, Browser) laden sich selbst neu, wenn TouchDeck am PC neu
      gestartet oder aktualisiert wurde, statt bis zum App-Neustart mit altem Code zu laufen.
    - Sicherheit: Seiten in Webseiten-Buttons können keine Popups oder neuen Fenster mehr öffnen, das
      Bedienfeld nicht wegnavigieren und das Programmfenster nicht dazu bringen, den Browser zu
      öffnen; das Programm öffnet den Browser höchstens einmal alle 3 Sekunden.
    - Webseiten-Buttons: das Programmfenster öffnet die Seite nicht mehr bei jeder Änderung im
      Browser, Webseiten-Buttons ohne Beschriftung sind im Bedienfeld nicht mehr unsichtbar, und der
      Editor warnt, wenn eine Seite das Einbetten verbietet. Adressen ohne „https://“ (z. B.
      www.beispiel.de) werden ergänzt statt verworfen.
    - „Ausführen“ im Vorlagen-Editor (und das Testen einzelner Aktionen darin) belegt die Eingaben
      mit ihren Standardwerten, {Eingabe.Name} ist dann nicht mehr leer.
    - Der Testmodus lädt Plugins auch aus ZIP-Dateien (Pfad oder „ZIP-Datei wählen …“), mit oder
      ohne Hauptordner im ZIP.
    - Profile, Icon-Pakete und Sicherungen lassen sich im Programmfenster exportieren: die Datei
      landet im Download-Ordner, wird im Dateimanager gezeigt und ihr Pfad angezeigt.
    - Benachrichtigungen, „Text tippen“, „Website öffnen“ und weitere Textaktionen setzen jetzt
      Platzhalter wie {Event.Durchlauf} oder {Eingabe.Name} ein (Shell-Befehle aus Sicherheitsgründen
      bewusst nicht).
    - Programme, Befehle und erweiterte Plugins (z. B. Python), die aus der AppImage gestartet
      werden, erben nicht mehr deren Bibliothekspfade (PYTHONHOME, LD_LIBRARY_PATH …) und laufen wie
      normal gestartet.
  Android-App:
    - App 0.6.3: eingebettete Seiten von Webseiten-Buttons bleiben im Button, statt den Browser zu
      öffnen; Links ohne passende App bringen die App nicht mehr zum Absturz.

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
    - Drehregler und Regler-Formen: Regler können Schieber (Fläche), Schieber mit Schiene und Griff,
      Drehregler (Ring) oder Drehregler mit Zeiger sein, mit eigener Farbe und Dicke. Drehregler
      dreht man im Kreis (unten links 0 %, unten rechts 100 %); sie steuern alles, was Regler können.
    - Import von Icon-Paketen: Eine Datei mit mehreren Paketen in Unterordnern (jedes mit eigener
      Beschreibung) wird zu mehreren Paketen; Links ohne „https://“ (z. B. „www.beispiel.de“)
      werden ergänzt, statt das Paket abzulehnen.
    - Deckkraft des Button-Hintergrunds (0–100 %) mit Knopf „Durchsichtig“: bei 0 % bleiben nur
      Symbol und Text, Schatten und Glanz fallen weg; Texturen behalten ihr Bild.
    - Icon-Pakete deutlich ausgebaut: Beschreibung, Version, Titelbild, Lizenztext und passende
      Button-Farbe pro Paket; Name und Schlagwörter pro Bild (im Paket anklicken); die
      Bildauswahl sucht in Namen und Schlagwörtern; .tdicons-Format 2 behält alles.
    - Der Import von Icon-Paketen erkennt den Inhalt statt der Dateiendung: ZIP-Dateien mit
      Bildern in Unterordnern und übliche Beschreibungsdateien (info.txt mit Schlüssel=Wert,
      manifest.json mit icons.json, Lizenzdateien) werden mit Namen, Schlagwörtern und Lizenz übernommen.
    - Optionale Fußleiste im Bedienfeld (Einstellungen › Bedienfeld › „Fußleiste anzeigen“):
      Verbindung mit dem Namen des PCs und in der Android-App ein Knopf „PC wechseln“.
    - Icon-Editor (Icon-Pakete › „Icon erstellen“): baut aus Hintergrund (Farbe, Verlauf oder
      durchsichtig; Quadrat, abgerundet oder Kreis), Emoji und Text ein PNG mit Live-Vorschau und
      speichert es bei den eigenen Bildern oder direkt in einem Icon-Paket.
    - Texturen für Buttons: eigenes Hintergrundbild (eigenes Bild oder aus einem Icon-Paket) unter
      Symbol und Beschriftung, dazu Overlays darüber: Glanz, Rahmen, Streifen, Punkte, Vignette,
      Leuchten. „Aussehen kopieren“ überträgt sie, Profile nehmen die Bilder mit.
    - Live-Anzeigen mit Zahl (CPU, RAM, Temperatur, Lautstärke, Timer, Zahlen-Werte) lassen sich
      zusätzlich als Fortschrittsbalken unten im Button zeigen.
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
    - Seiten-Import aus einem anderen Programm entfernt; er diente nur zum Testen.
    - Der Hinweis zum Querhalten spricht vom „Gerät“ statt vom „Handy“ und blendet sich nach ein
      paar Sekunden aus; beim Öffnen einer anderen breiten Seite erscheint er wieder.
    - Auswahl eines Buttons aufheben ist einfacher: Klick neben die Seite oder auf das neue „✕“
      in den Button-Eigenschaften (Esc geht weiter) – dann erscheinen die Seiten-Einstellungen.
    - Die Seiten-Einstellungen (wenn kein Button gewählt ist) sind in die Reiter „Logik“ und
      „Raster“ / „Fläche“ aufgeteilt, die Logik zuerst.
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
    - Beim Tippen in der Bildsuche geht der Fokus nicht mehr nach jedem Buchstaben verloren (eine
      folgende Rücktaste konnte den ausgewählten Button löschen).
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
  Modding:
    - Plugin-Schnittstelle erweitert: Plugins können auf TouchDeck-Ereignisse reagieren (td.on,
      z. B. Seite geöffnet, stummgeschaltet, Programm im Vordergrund, Timer abgelaufen), Seiten
      auf allen Geräten anzeigen, das Aussehen von Buttons ändern und zurücksetzen, Timer steuern
      und verbundene Geräte sowie den Zustand von OBS, Twitch und Discord lesen. Neue
      Berechtigungen „events“, „deck“ und „status“.
    - Plugins können eigene Live-Anzeigen mitbringen („lives“ in plugin.json) mit Text,
      Fortschrittsbalken und Aussehen „an“; sie erscheinen in der Liste der Live-Anzeigen.
    - Erweiterte Plugins bekommen alle Ereignisse und nutzen dieselben Funktionen über
      „call“ / „reply“.
    - Plugin-Aktionen und -Auslöser funktionieren auch beim Doppelt-Drücken und in der
      Seiten-Logik.
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

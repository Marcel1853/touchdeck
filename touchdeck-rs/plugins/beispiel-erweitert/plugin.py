#!/usr/bin/env python3
"""Beispiel für ein erweitertes TouchDeck-Plugin (volle Rechte, nur im erweiterten Modus).

TouchDeck startet dieses Programm selbst und spricht mit ihm über JSON-Zeilen:
Nachrichten von TouchDeck kommen auf stdin, Antworten gehen auf stdout (eine Zeile je Nachricht).
Alles auf stderr erscheint im TouchDeck-Protokoll.
"""
import json
import subprocess
import sys
import threading
import time

lock = threading.Lock()
settings = {}


def send(msg):
    with lock:
        sys.stdout.write(json.dumps(msg, ensure_ascii=False) + "\n")
        sys.stdout.flush()


def log(text, level="info"):
    send({"type": "log", "level": level, "message": text})


def load_average():
    with open("/proc/loadavg", encoding="utf-8") as f:
        return float(f.read().split()[0])


def action_auslastung(params):
    return f"Auslastung: {load_average():.2f}"


def action_befehl(params):
    cmd = params.get("befehl") or "uptime -p"
    out = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=20)
    text = (out.stdout or out.stderr).strip()[:500]
    if params.get("wert"):
        send({"type": "setValue", "name": params["wert"], "value": text})
    return text


ACTIONS = {"auslastung": action_auslastung, "befehl": action_befehl}


def watch():
    """Läuft im Hintergrund: schreibt die Auslastung in einen Wert und meldet hohe Last."""
    was_high = False
    while True:
        load = load_average()
        send({"type": "setValue", "name": "System.Auslastung", "value": f"{load:.2f}"})
        high = load >= float(settings.get("grenze") or 4)
        if high and not was_high:
            send({"type": "fire", "trigger": "hoch", "data": {"Auslastung": f"{load:.2f}"}})
        was_high = high
        time.sleep(10)


for line in sys.stdin:
    msg = json.loads(line)
    kind = msg.get("type")
    if kind == "start":
        settings = msg.get("settings") or {}
        threading.Thread(target=watch, daemon=True).start()
        log(f"Python-Beispiel gestartet (TouchDeck {msg.get('touchdeck')})")
    elif kind == "action":
        try:
            result = ACTIONS[msg["id"]](msg.get("params") or {})
            send({"type": "result", "requestId": msg["requestId"], "ok": True, "message": result})
        except Exception as err:  # Fehler an TouchDeck melden statt abzustürzen
            send({"type": "result", "requestId": msg["requestId"], "ok": False, "error": str(err)})
    elif kind == "stop":
        break

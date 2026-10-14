#!/usr/bin/env python3
"""Tastatur, Maus und Fenster unter X11 steuern (XTest und EWMH).

Benötigt nur libX11 und libXtst, die auf jedem X11-Desktop vorhanden sind.

Aufruf:
  xinput.py hotkey ctrl+alt+t
  xinput.py type "Hallo Welt"
  xinput.py hold ctrl+w 1500          Taste(n) 1,5 s gedrückt halten
  xinput.py click left|middle|right|double|scrollup|scrolldown
  xinput.py move 500 300              Maus an Position setzen
  xinput.py moverel 50 -20            Maus relativ bewegen
  xinput.py windows                   offene Fenster als JSON
  xinput.py activate firefox          Fenster nach Klasse oder Titel nach vorn holen
"""
import ctypes
import json
import sys
import time

x11 = ctypes.cdll.LoadLibrary("libX11.so.6")
xtst = ctypes.cdll.LoadLibrary("libXtst.so.6")

KeySym = ctypes.c_ulong
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
x11.XStringToKeysym.restype = KeySym
x11.XStringToKeysym.argtypes = [ctypes.c_char_p]
x11.XKeysymToKeycode.restype = ctypes.c_ubyte
x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, KeySym]
x11.XDisplayKeycodes.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int)]
x11.XGetKeyboardMapping.restype = ctypes.POINTER(KeySym)
x11.XGetKeyboardMapping.argtypes = [ctypes.c_void_p, ctypes.c_ubyte, ctypes.c_int, ctypes.POINTER(ctypes.c_int)]
x11.XChangeKeyboardMapping.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.POINTER(KeySym), ctypes.c_int]
x11.XFree.argtypes = [ctypes.c_void_p]
x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
x11.XFlush.argtypes = [ctypes.c_void_p]
xtst.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
xtst.XTestFakeRelativeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_ulong]
x11.XDefaultRootWindow.restype = ctypes.c_ulong
x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x11.XInternAtom.restype = ctypes.c_ulong
x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
x11.XGetWindowProperty.argtypes = [
    ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long, ctypes.c_long, ctypes.c_int, ctypes.c_ulong,
    ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong),
    ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ubyte)),
]
x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.c_void_p]


class XClientMessageEvent(ctypes.Structure):
    _fields_ = [
        ("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int),
        ("display", ctypes.c_void_p), ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong),
        ("format", ctypes.c_int), ("l", ctypes.c_long * 5),
    ]


class XEvent(ctypes.Union):
    _fields_ = [("xclient", XClientMessageEvent), ("pad", ctypes.c_long * 24)]


CLIENT_MESSAGE = 33
SUBSTRUCTURE_REDIRECT_AND_NOTIFY = (1 << 20) | (1 << 19)
BUTTONS = {"left": 1, "middle": 2, "right": 3, "scrollup": 4, "scrolldown": 5}

# Namen, wie sie der Editor erzeugt → X11-Keysym-Namen.
NAMES = {
    "ctrl": "Control_L", "control": "Control_L", "shift": "Shift_L", "alt": "Alt_L",
    "altgr": "ISO_Level3_Shift", "super": "Super_L", "meta": "Super_L", "win": "Super_L",
    "enter": "Return", "return": "Return", "esc": "Escape", "escape": "Escape",
    "space": "space", "tab": "Tab", "backspace": "BackSpace", "delete": "Delete", "del": "Delete",
    "insert": "Insert", "home": "Home", "end": "End", "pageup": "Prior", "pagedown": "Next",
    "up": "Up", "down": "Down", "left": "Left", "right": "Right",
    "print": "Print", "printscreen": "Print", "pause": "Pause", "menu": "Menu",
    "capslock": "Caps_Lock", "numlock": "Num_Lock", "scrolllock": "Scroll_Lock",
    "plus": "plus", "minus": "minus", "comma": "comma", "period": "period",
    "playpause": "XF86AudioPlay", "nexttrack": "XF86AudioNext", "prevtrack": "XF86AudioPrev",
    "stop": "XF86AudioStop", "volumeup": "XF86AudioRaiseVolume",
    "volumedown": "XF86AudioLowerVolume", "volumemute": "XF86AudioMute",
}
SPECIAL_CHARS = {"\n": "Return", "\t": "Tab", " ": "space"}

SHIFT = "Shift_L"
LEVEL3 = "ISO_Level3_Shift"


class Keyboard:
    def __init__(self):
        self.dpy = x11.XOpenDisplay(None)
        if not self.dpy:
            raise SystemExit("Keine Verbindung zum X-Server (DISPLAY nicht gesetzt?)")
        lo, hi = ctypes.c_int(), ctypes.c_int()
        x11.XDisplayKeycodes(self.dpy, ctypes.byref(lo), ctypes.byref(hi))
        self.lo, self.hi = lo.value, hi.value
        per = ctypes.c_int()
        count = self.hi - self.lo + 1
        mapping = x11.XGetKeyboardMapping(self.dpy, self.lo, count, ctypes.byref(per))
        self.per = per.value
        self.map = [[mapping[i * self.per + j] for j in range(self.per)] for i in range(count)]
        x11.XFree(mapping)
        self.spare = next((self.lo + i for i, syms in enumerate(self.map) if not any(syms)), None)

    def keysym(self, name):
        sym = x11.XStringToKeysym(name.encode())
        if not sym:
            raise SystemExit(f"Unbekannte Taste: {name}")
        return sym

    def locate(self, sym):
        """Liefert (Keycode, nötige Umschalttasten) für ein Keysym."""
        levels = {0: [], 1: [SHIFT], 4: [LEVEL3], 5: [LEVEL3, SHIFT]}
        for idx, mods in levels.items():
            for i, syms in enumerate(self.map):
                if idx < len(syms) and syms[idx] == sym:
                    return self.lo + i, mods
        return None, []

    def tap(self, code, down=True, up=True):
        if down:
            xtst.XTestFakeKeyEvent(self.dpy, code, True, 0)
        if up:
            xtst.XTestFakeKeyEvent(self.dpy, code, False, 0)
        x11.XFlush(self.dpy)

    def with_temp_code(self, sym, fn):
        """Belegt kurzzeitig einen freien Keycode, falls ein Zeichen nicht im Layout ist."""
        if self.spare is None:
            raise SystemExit("Kein freier Keycode für Sonderzeichen gefunden")
        arr = (KeySym * self.per)(*([sym, sym] + [0] * (self.per - 2)))
        x11.XChangeKeyboardMapping(self.dpy, self.spare, self.per, arr, 1)
        x11.XSync(self.dpy, False)
        time.sleep(0.03)
        try:
            fn(self.spare)
        finally:
            x11.XSync(self.dpy, False)
            time.sleep(0.03)
            empty = (KeySym * self.per)()
            x11.XChangeKeyboardMapping(self.dpy, self.spare, self.per, empty, 1)
            x11.XSync(self.dpy, False)

    def char_sym(self, ch):
        cp = ord(ch)
        return cp if (0x20 <= cp <= 0x7E or 0xA0 <= cp <= 0xFF) else 0x01000000 | cp

    def name_sym(self, part):
        """Ein Teil eines Kürzels: einzelnes Zeichen („a“, „#“, „ü“) oder Tastenname („enter“, „F5“)."""
        if len(part) == 1:
            return self.char_sym(part.lower())
        name = NAMES.get(part.lower(), part)
        if len(name) <= 3 and name[0] in "fF" and name[1:].isdigit():
            name = name.upper()
        return self.keysym(name)

    def press_sym(self, sym, extra_mods=()):
        """Drückt ein Keysym, gehalten zusammen mit den Modifikator-Keysyms extra_mods."""
        code, level_mods = self.locate(sym)
        level_syms = [self.keysym(m) for m in level_mods]
        mods = list(dict.fromkeys([*extra_mods, *level_syms]))

        def run(c, mod_syms):
            mod_codes = [self.locate(m)[0] for m in mod_syms]
            for m in mod_codes:
                self.tap(m, up=False)
            self.tap(c)
            for m in reversed(mod_codes):
                self.tap(m, down=False)

        if code is None:
            self.with_temp_code(sym, lambda c: run(c, list(extra_mods)))
        else:
            run(code, mods)

    def hotkey(self, combo):
        parts = [p.strip() for p in combo.split("+") if p.strip()]
        if not parts:
            raise SystemExit("Leeres Tastenkürzel")
        syms = [self.name_sym(p) for p in parts]
        self.press_sym(syms[-1], extra_mods=syms[:-1])

    def type_text(self, text, delay=0.008):
        for ch in text:
            sym = self.keysym(SPECIAL_CHARS[ch]) if ch in SPECIAL_CHARS else self.char_sym(ch)
            self.press_sym(sym)
            time.sleep(delay)

    def hold(self, combo, ms):
        """Wie hotkey, hält die Tasten aber für ms Millisekunden gedrückt."""
        syms = [self.name_sym(p.strip()) for p in combo.split("+") if p.strip()]
        codes = [self.locate(s)[0] for s in syms]
        if None in codes:
            raise SystemExit("Taste ist im aktuellen Layout nicht vorhanden")
        for c in codes:
            self.tap(c, up=False)
        time.sleep(max(0, min(60000, ms)) / 1000)
        for c in reversed(codes):
            self.tap(c, down=False)

    def click(self, which):
        if which == "double":
            for _ in range(2):
                self.button(1)
                time.sleep(0.06)
            return
        if which not in BUTTONS:
            raise SystemExit(f"Unbekannte Maustaste: {which}")
        self.button(BUTTONS[which])

    def button(self, n):
        xtst.XTestFakeButtonEvent(self.dpy, n, True, 0)
        xtst.XTestFakeButtonEvent(self.dpy, n, False, 0)
        x11.XFlush(self.dpy)

    def move(self, x, y, relative=False):
        if relative:
            xtst.XTestFakeRelativeMotionEvent(self.dpy, x, y, 0)
        else:
            xtst.XTestFakeMotionEvent(self.dpy, -1, x, y, 0)
        x11.XFlush(self.dpy)

    # ---------- Fenster (EWMH) ----------

    def atom(self, name):
        return x11.XInternAtom(self.dpy, name.encode(), False)

    def prop(self, win, name, kind=0):
        """Liest eine Fenster-Eigenschaft als Bytes (kind 0 = beliebiger Typ)."""
        rtype, rfmt = ctypes.c_ulong(), ctypes.c_int()
        n, after = ctypes.c_ulong(), ctypes.c_ulong()
        data = ctypes.POINTER(ctypes.c_ubyte)()
        ok = x11.XGetWindowProperty(self.dpy, win, self.atom(name), 0, 65536, False, kind,
                                    ctypes.byref(rtype), ctypes.byref(rfmt), ctypes.byref(n), ctypes.byref(after), ctypes.byref(data))
        if ok != 0 or not data:
            return None, 0, 0
        size = {8: 1, 16: 2, 32: ctypes.sizeof(ctypes.c_long)}.get(rfmt.value, 1) * n.value
        raw = ctypes.string_at(data, size)
        x11.XFree(data)
        return raw, rfmt.value, n.value

    def windows(self):
        root = x11.XDefaultRootWindow(self.dpy)
        raw, fmt, n = self.prop(root, "_NET_CLIENT_LIST")
        if not raw:
            return []
        ids = (ctypes.c_ulong * n).from_buffer_copy(raw)
        result = []
        for wid in ids:
            title = (self.prop(wid, "_NET_WM_NAME")[0] or self.prop(wid, "WM_NAME")[0] or b"").decode("utf-8", "replace")
            cls = (self.prop(wid, "WM_CLASS")[0] or b"").split(b"\0")
            result.append({"id": wid, "class": (cls[1] if len(cls) > 1 else cls[0]).decode("utf-8", "replace"), "title": title})
        return result

    def activate(self, query):
        q = query.lower()
        wins = self.windows()
        match = next((w for w in wins if w["class"].lower() == q), None) \
            or next((w for w in wins if q in w["class"].lower()), None) \
            or next((w for w in wins if q in w["title"].lower()), None)
        if not match:
            raise SystemExit(f"Kein offenes Fenster gefunden für „{query}“")
        ev = XEvent()
        ev.xclient.type = CLIENT_MESSAGE
        ev.xclient.send_event = True
        ev.xclient.window = match["id"]
        ev.xclient.message_type = self.atom("_NET_ACTIVE_WINDOW")
        ev.xclient.format = 32
        ev.xclient.l[0] = 2  # Quelle: Werkzeug wie Pager/Taskleiste
        root = x11.XDefaultRootWindow(self.dpy)
        x11.XSendEvent(self.dpy, root, False, SUBSTRUCTURE_REDIRECT_AND_NOTIFY, ctypes.byref(ev))
        x11.XFlush(self.dpy)
        print(match["title"])

    def close(self):
        x11.XSync(self.dpy, False)
        x11.XCloseDisplay(self.dpy)


def main():
    args = sys.argv[1:]
    commands = {
        "hotkey": (1, lambda kb, a: kb.hotkey(a[0])),
        "type": (1, lambda kb, a: kb.type_text(a[0])),
        "hold": (2, lambda kb, a: kb.hold(a[0], int(a[1]))),
        "click": (1, lambda kb, a: kb.click(a[0])),
        "move": (2, lambda kb, a: kb.move(int(a[0]), int(a[1]))),
        "moverel": (2, lambda kb, a: kb.move(int(a[0]), int(a[1]), relative=True)),
        "windows": (0, lambda kb, a: print(json.dumps(kb.windows(), ensure_ascii=False))),
        "activate": (1, lambda kb, a: kb.activate(a[0])),
    }
    if not args or args[0] not in commands or len(args) - 1 != commands[args[0]][0]:
        raise SystemExit(__doc__)
    kb = Keyboard()
    try:
        commands[args[0]][1](kb, args[1:])
    finally:
        kb.close()


if __name__ == "__main__":
    main()

// Desktop-Programm: startet den TouchDeck-Server, zeigt den Editor im eigenen Fenster
// und bleibt über das Symbol in der Systemleiste im Hintergrund erreichbar.
import { app, BrowserWindow, Tray, Menu, Notification, shell, dialog, nativeImage, utilityProcess } from 'electron';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PORT = Number.parseInt(process.env.TOUCHDECK_PORT, 10) || 8770;
const BASE = `http://localhost:${PORT}`;
const AUTOSTART_FILE = path.join(process.env.XDG_CONFIG_HOME || path.join(os.homedir(), '.config'), 'autostart', 'touchdeck.desktop');

let win = null;
let tray = null;
let quitting = false;
let hintShown = false;
let server = null;
let restarts = [];

// ---------- Fenster ----------

function createWindow() {
  win = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1000,
    minHeight: 640,
    show: false,
    title: 'TouchDeck',
    icon: path.join(HERE, 'tray.png'),
    backgroundColor: '#0d0f14',
    autoHideMenuBar: true,
    webPreferences: { contextIsolation: true, sandbox: true },
  });
  win.setMenuBarVisibility(false);
  win.loadURL(`${BASE}/editor/`);
  win.once('ready-to-show', () => win.show());

  // Schließen versteckt nur das Fenster: der Server läuft weiter, damit das Handy verbunden bleibt.
  win.on('close', (e) => {
    if (quitting) return;
    e.preventDefault();
    win.hide();
    if (!hintShown && Notification.isSupported()) {
      hintShown = true;
      new Notification({ title: 'TouchDeck läuft weiter', body: 'Über das Symbol in der Systemleiste öffnest du den Editor wieder oder beendest TouchDeck.' }).show();
    }
  });
  // Ungespeicherte Änderungen werden ohnehin nach 0,4 s gespeichert – Beenden nicht blockieren.
  win.webContents.on('will-prevent-unload', (e) => e.preventDefault());

  // Vorschau im eigenen kleinen Fenster, alle anderen Links im normalen Browser.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith(`${BASE}/deck/`)) {
      return { action: 'allow', overrideBrowserWindowOptions: { width: 480, height: 820, title: 'TouchDeck – Vorschau', autoHideMenuBar: true, backgroundColor: '#0d0f14', icon: path.join(HERE, 'tray.png') } };
    }
    shell.openExternal(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (e, url) => {
    if (!url.startsWith(`${BASE}/`)) {
      e.preventDefault();
      shell.openExternal(url);
    }
  });
}

function showEditor(hash = '') {
  if (!win || win.isDestroyed()) createWindow();
  if (hash) win.webContents.executeJavaScript(`location.hash = ${JSON.stringify(hash)}`).catch(() => {});
  win.show();
  win.focus();
}

function openPreview() {
  showEditor();
  win.webContents.executeJavaScript(`window.open('/deck/', 'touchdeck-vorschau')`).catch(() => {});
}

// ---------- Autostart ----------

function launchCommand() {
  if (process.env.APPIMAGE) return `"${process.env.APPIMAGE}" --hidden`;
  if (app.isPackaged) return `"${process.execPath}" --hidden`;
  return `"${process.execPath}" "${app.getAppPath()}" --hidden`;
}

const autostartEnabled = () => fs.existsSync(AUTOSTART_FILE);

function setAutostart(on) {
  if (!on) {
    fs.rmSync(AUTOSTART_FILE, { force: true });
    return;
  }
  fs.mkdirSync(path.dirname(AUTOSTART_FILE), { recursive: true });
  fs.writeFileSync(AUTOSTART_FILE, [
    '[Desktop Entry]',
    'Type=Application',
    'Name=TouchDeck',
    'Comment=Makro-Fernbedienung im Hintergrund starten',
    `Exec=${launchCommand()}`,
    'Icon=touchdeck',
    'Terminal=false',
    'X-GNOME-Autostart-enabled=true',
    '',
  ].join('\n'));
}

// ---------- Systemleiste ----------

function updateTrayMenu() {
  tray.setContextMenu(Menu.buildFromTemplate([
    { label: 'Editor öffnen', click: () => showEditor() },
    { label: 'Handy verbinden …', click: () => showEditor('pair') },
    { label: 'Vorschau öffnen', click: openPreview },
    { type: 'separator' },
    {
      label: 'Beim Anmelden starten',
      type: 'checkbox',
      checked: autostartEnabled(),
      click: (item) => {
        setAutostart(item.checked);
        updateTrayMenu();
      },
    },
    { type: 'separator' },
    { label: 'TouchDeck beenden', click: () => { quitting = true; app.quit(); } },
  ]));
}

function createTray() {
  const icon = nativeImage.createFromPath(path.join(HERE, 'tray.png')).resize({ width: 24, height: 24 });
  tray = new Tray(icon);
  tray.setToolTip('TouchDeck');
  tray.on('click', () => showEditor());
  updateTrayMenu();
}

// ---------- Server-Prozess ----------

const dataDir = () => process.env.TOUCHDECK_DATA || (app.isPackaged ? path.join(app.getPath('userData'), 'data') : path.join(HERE, '..', 'data'));

/** Startet den Server als eigenen Prozess; löst auf, sobald er lauscht. */
function startServer() {
  return new Promise((resolve, reject) => {
    const env = { ...process.env, TOUCHDECK_PORT: String(PORT), TOUCHDECK_DATA: dataDir() };
    delete env.ELECTRON_RUN_AS_NODE;
    server = utilityProcess.fork(path.join(HERE, 'server-process.js'), [], { env, serviceName: 'TouchDeck-Server', stdio: 'inherit' });
    let started = false;
    server.on('message', (msg) => {
      if (msg.type === 'pair-request') {
        showEditor();
        win?.flashFrame(true);
      } else if (msg.type === 'ready') {
        started = true;
        console.log(`TouchDeck läuft auf ${BASE} – Daten: ${msg.dataDir}`);
        resolve();
      } else if (msg.type === 'error') {
        reject(Object.assign(new Error(msg.message), { code: msg.code }));
      }
    });
    server.on('exit', (code) => {
      server = null;
      if (!started || quitting) return;
      // Unerwartet beendet: neu starten, aber höchstens 3-mal pro Minute.
      restarts = restarts.filter((t) => Date.now() - t < 60000);
      if (restarts.length >= 3) {
        dialog.showErrorBox('TouchDeck', `Der TouchDeck-Server ist mehrfach abgestürzt (Code ${code}). Bitte TouchDeck neu starten.`);
        return;
      }
      restarts.push(Date.now());
      startServer().then(() => win && !win.isDestroyed() && win.webContents.reload()).catch(() => {});
    });
  });
}

// ---------- Start ----------

async function start() {
  try {
    await startServer();
  } catch (err) {
    dialog.showErrorBox('TouchDeck kann nicht starten', err.code === 'EADDRINUSE'
      ? `Port ${PORT} ist schon belegt. Läuft TouchDeck bereits, z. B. aus dem Terminal gestartet? Diese Instanz bitte zuerst beenden.`
      : err.message);
    quitting = true;
    app.quit();
    return;
  }
  createTray();
  if (!process.argv.includes('--hidden')) showEditor();
}

// Eigenes Profil (z. B. für Tests neben einer laufenden Installation).
if (process.env.TOUCHDECK_PROFILE) app.setPath('userData', process.env.TOUCHDECK_PROFILE);

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on('second-instance', () => showEditor());
  // Ohne offenes Fenster weiterlaufen – das Symbol in der Systemleiste hält das Programm.
  app.on('window-all-closed', () => {});
  app.on('before-quit', () => {
    quitting = true;
    server?.kill();
  });
  app.whenReady().then(start);
}

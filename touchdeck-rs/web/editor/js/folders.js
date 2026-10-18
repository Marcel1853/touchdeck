// Ordner für Seiten und Vorlagen: eigene Ordner (auch leere), einklappbar, Mehrfachauswahl
// (Strg/Shift + Klick), Einträge per Ziehen oder Rechtsklick in Ordner verschieben.
import { t as tr } from '/shared/i18n.js';
import { h, prompt, prefs, contextMenu } from './dom.js';
import { S, mutate, render } from './state.js';

// Ausgewählte Einträge je Liste (Ids).
const selection = { pages: new Set(), flows: new Set(), events: new Set() };
const anchor = { pages: null, flows: null, events: null };

const collapsedKey = (kind) => `touchdeck.folders.${kind}.collapsed`;
function collapsed(kind) {
  try {
    return new Set(JSON.parse(prefs.get(collapsedKey(kind)) || '[]'));
  } catch {
    return new Set();
  }
}
function setCollapsed(kind, set) {
  prefs.set(collapsedKey(kind), JSON.stringify([...set]));
}

/** Alle Ordner einer Liste: angelegte (auch leere) und solche, die nur an Einträgen stehen. */
export function folderNames(kind, items) {
  S.folders ??= { pages: [], flows: [], events: [] };
  S.folders[kind] ??= [];
  const names = [...(S.folders[kind] ?? [])];
  for (const it of items) if (it.folder && !names.includes(it.folder)) names.push(it.folder);
  return names;
}

/** Ausgewählte Einträge; ohne Auswahl der eine Eintrag. */
export function selectedOr(kind, items, item) {
  const sel = selection[kind];
  return sel.has(item.id) && sel.size > 1 ? items.filter((x) => sel.has(x.id)) : [item];
}

export function clearSelection(kind) {
  selection[kind].clear();
}

export async function addFolder(kind) {
  const name = (await prompt(tr('folders.newFolder'), tr('folders.folderName')))?.slice(0, 60).trim();
  if (!name) return null;
  mutate(() => {
    S.folders ??= { pages: [], flows: [], events: [] };
  S.folders[kind] ??= [];
    if (!S.folders[kind].includes(name)) S.folders[kind].push(name);
  });
  return name;
}

/** Menüpunkt „In Ordner verschieben ▸“ mit allen Ordnern, „Neuer Ordner …“ und „Ohne Ordner“. */
export function moveToFolderItem(kind, allItems, items) {
  const move = (name) => mutate(() => items.forEach((x) => (x.folder = name)));
  return {
    icon: '📁', label: tr('folders.moveToFolder'),
    submenu: [
      ...folderNames(kind, allItems).map((f) => ({ icon: '📁', label: f, disabled: items.every((x) => x.folder === f), onclick: () => move(f) })),
      { icon: '＋', label: tr('folders.newFolder'), onclick: async () => {
        const name = await addFolder(kind);
        if (name) move(name);
      } },
      '-',
      { icon: '↩', label: tr('folders.noFolder'), disabled: items.every((x) => !x.folder), onclick: () => move('') },
    ],
  };
}

/**
 * Zeichnet eine Liste mit Ordnern in ul.
 * opts: kind ('pages' | 'flows'), items (Array, Reihenfolge = gespeicherte Reihenfolge), activeId,
 * open(item), row(item) → Inhalt der Zeile, menu(items, x, y) → Rechtsklick auf Einträge,
 * deleteItems(items) → Einträge löschen (für „Ordner mit Inhalt löschen“).
 */
export function renderFolderList(ul, { kind, items, activeId, open, row, menu, deleteItems }) {
  const sel = selection[kind];
  for (const id of [...sel]) if (!items.some((x) => x.id === id)) sel.delete(id);
  const closed = collapsed(kind);
  const folders = folderNames(kind, items);
  // Anzeige-Reihenfolge: erst Einträge ohne Ordner, dann je Ordner.
  const order = [...items.filter((x) => !x.folder), ...folders.flatMap((f) => items.filter((x) => x.folder === f))];
  let dragId = null;

  const click = (e, it) => {
    if (e.ctrlKey || e.metaKey) {
      sel.has(it.id) ? sel.delete(it.id) : sel.add(it.id);
      anchor[kind] = it.id;
      return render({ detail: false });
    }
    if (e.shiftKey) {
      const a = order.findIndex((x) => x.id === (anchor[kind] ?? activeId));
      const b = order.findIndex((x) => x.id === it.id);
      sel.clear();
      for (let i = Math.min(a, b); i <= Math.max(a, b); i++) if (i >= 0) sel.add(order[i].id);
      return render({ detail: false });
    }
    sel.clear();
    anchor[kind] = it.id;
    open(it);
  };
  const dropOnItem = (e, target) => {
    e.preventDefault();
    const moving = dragId && (sel.has(dragId) ? items.filter((x) => sel.has(x.id)) : items.filter((x) => x.id === dragId));
    if (!moving?.length || moving.some((x) => x.id === target.id)) return render({ detail: false });
    mutate(() => {
      for (const m of moving) {
        items.splice(items.indexOf(m), 1);
        m.folder = target.folder ?? '';
      }
      items.splice(items.indexOf(target), 0, ...moving);
    }, { detail: false });
  };
  const dropOnFolder = (e, f) => {
    e.preventDefault();
    const moving = dragId && (sel.has(dragId) ? items.filter((x) => sel.has(x.id)) : items.filter((x) => x.id === dragId));
    if (moving?.length) mutate(() => moving.forEach((x) => (x.folder = f)), { detail: false });
  };
  const dragOver = (e) => {
    if (!dragId) return;
    e.preventDefault();
    e.currentTarget.classList.add('drag-over');
  };
  const itemLi = (it, inFolder) => h('li', {
    class: `${it.id === activeId ? 'active' : ''}${sel.has(it.id) ? ' selected' : ''}${inFolder ? ' in-folder' : ''}`,
    draggable: 'true',
    title: tr('folders.itemHint'),
    onclick: (e) => click(e, it),
    oncontextmenu: (e) => {
      e.preventDefault();
      if (!sel.has(it.id)) sel.clear();
      menu(selectedOr(kind, items, it), e.clientX, e.clientY);
    },
    ondragstart: (e) => {
      dragId = it.id;
      e.dataTransfer.setData('text/x-folder-item', it.id);
    },
    ondragend: () => (dragId = null),
    ondragover: dragOver,
    ondragleave: (e) => e.currentTarget.classList.remove('drag-over'),
    ondrop: (e) => dropOnItem(e, it),
  }, ...row(it));
  const folderLi = (f) => {
    const list = items.filter((x) => x.folder === f);
    const isClosed = closed.has(f);
    return h('li', {
      class: 'folder-head',
      onclick: () => {
        isClosed ? closed.delete(f) : closed.add(f);
        setCollapsed(kind, closed);
        render({ detail: false });
      },
      oncontextmenu: (e) => {
        e.preventDefault();
        folderMenu(kind, items, f, e.clientX, e.clientY, deleteItems);
      },
      ondragover: dragOver,
      ondragleave: (e) => e.currentTarget.classList.remove('drag-over'),
      ondrop: (e) => dropOnFolder(e, f),
    }, h('span', { class: 'chevron' }, isClosed ? '▸' : '▾'), h('span', { class: 'name' }, `📁 ${f}`), h('span', { class: 'count' }, String(list.length)));
  };
  ul.replaceChildren(
    ...items.filter((x) => !x.folder).map((it) => itemLi(it, false)),
    ...folders.flatMap((f) => [folderLi(f), ...(closed.has(f) ? [] : items.filter((x) => x.folder === f).map((it) => itemLi(it, true)))]),
  );
}

/** Rechtsklick auf einen Ordner: umbenennen, auflösen (Inhalt bleibt) oder samt Inhalt löschen. */
function folderMenu(kind, items, f, x, y, deleteItems) {
  const inside = items.filter((it) => it.folder === f);
  contextMenu(x, y, [
    { icon: '✏️', label: tr('folders.rename'), onclick: async () => {
      const name = (await prompt(tr('folders.rename'), tr('folders.folderName'), f))?.slice(0, 60).trim();
      if (!name || name === f) return;
      mutate(() => {
        inside.forEach((it) => (it.folder = name));
        const list = S.folders[kind];
        const i = list.indexOf(f);
        if (i >= 0) list[i] = name;
        else if (!list.includes(name)) list.push(name);
      });
    } },
    { icon: '☑', label: tr('folders.selectAll'), disabled: !inside.length, onclick: () => {
      selection[kind] = new Set(inside.map((it) => it.id));
      render({ detail: false });
    } },
    '-',
    { icon: '📤', label: tr('folders.dissolve'), onclick: () => mutate(() => {
      inside.forEach((it) => (it.folder = ''));
      S.folders[kind] = S.folders[kind].filter((n) => n !== f);
    }) },
    { icon: '🗑', label: tr('folders.deleteWithContents', [inside.length]), danger: true, disabled: !inside.length, onclick: async () => {
      if (await deleteItems(inside) !== false) mutate(() => (S.folders[kind] = S.folders[kind].filter((n) => n !== f)));
    } },
  ]);
}

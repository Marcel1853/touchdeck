// Kleiner Markdown-Leser für Plugin-Beschreibungen (README). Baut nur DOM-Elemente mit Text –
// HTML aus der README wird nie übernommen, Links und Bilder nur über http(s).
import { h } from './dom.js';

/** Adresse aus der README: relative Pfade gegen base (Ordner des Plugins), nur http(s). */
function resolve(url, base) {
  try {
    const u = new URL(url, base);
    return u.protocol === 'https:' || u.protocol === 'http:' ? u.href : null;
  } catch {
    return null;
  }
}

/** Zeilen-Inhalt: `Code`, **fett**, *kursiv*, [Link](url), ![Bild](url). */
function inline(text, base) {
  const out = [];
  const re = /(!?)\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)|`([^`]+)`|\*\*([^*]+)\*\*|__([^_]+)__|\*([^*\s][^*]*)\*|_([^_\s][^_]*)_/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    if (m.index > last) out.push(text.slice(last, m.index));
    last = m.index + m[0].length;
    if (m[3] !== undefined) {
      const url = resolve(m[3], base);
      if (m[1]) out.push(url ? h('img', { src: url, alt: m[2], loading: 'lazy' }) : m[2]);
      else if (m[3].startsWith('#') || !url) out.push(...inline(m[2], base));
      else out.push(h('a', { href: url, target: '_blank', rel: 'noopener noreferrer' }, ...inline(m[2], base)));
    } else if (m[4] !== undefined) out.push(h('code', {}, m[4]));
    else if (m[5] !== undefined || m[6] !== undefined) out.push(h('strong', {}, ...inline(m[5] ?? m[6], base)));
    else out.push(h('em', {}, ...inline(m[7] ?? m[8], base)));
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

const cells = (line) => line.trim().replace(/^\||\|$/g, '').split('|').map((c) => c.trim());

/** Markdown-Text als DOM-Element. base = Ordner-Adresse für relative Links und Bilder. */
export function renderMarkdown(text, base = '') {
  const root = h('div', { class: 'markdown' });
  const lines = text.replace(/\r/g, '').replace(/<!--[\s\S]*?-->/g, '').split('\n');
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    // Codeblock
    if (/^\s*```/.test(line)) {
      const code = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i++]);
      i++;
      root.append(h('pre', { class: 'code' }, h('code', {}, code.join('\n'))));
      continue;
    }
    const head = /^(#{1,6})\s+(.*)$/.exec(line);
    if (head) {
      root.append(h(`h${Math.min(6, head[1].length + 1)}`, {}, ...inline(head[2].replace(/\s*#+\s*$/, ''), base)));
      i++;
      continue;
    }
    if (/^\s*([-*_])\s*\1\s*\1[\s\1]*$/.test(line)) {
      root.append(h('hr'));
      i++;
      continue;
    }
    // Tabelle: Kopfzeile, Trennzeile mit ---, dann Zeilen
    if (line.includes('|') && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1] ?? '')) {
      const header = cells(line);
      i += 2;
      const rows = [];
      while (i < lines.length && lines[i].includes('|') && lines[i].trim()) rows.push(cells(lines[i++]));
      root.append(h('table', {}, h('thead', {}, h('tr', {}, header.map((c) => h('th', {}, ...inline(c, base))))),
        h('tbody', {}, rows.map((r) => h('tr', {}, r.map((c) => h('td', {}, ...inline(c, base))))))));
      continue;
    }
    if (/^\s*>/.test(line)) {
      const quote = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) quote.push(lines[i++].replace(/^\s*>\s?/, ''));
      root.append(h('blockquote', {}, renderMarkdown(quote.join('\n'), base)));
      continue;
    }
    const item = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/;
    if (item.test(line)) {
      const ordered = /\d/.test(item.exec(line)[2]);
      const list = h(ordered ? 'ol' : 'ul');
      while (i < lines.length && item.test(lines[i])) {
        let content = item.exec(lines[i++])[3];
        // eingerückte Folgezeilen gehören zum Punkt
        while (i < lines.length && /^\s{2,}\S/.test(lines[i]) && !item.test(lines[i])) content += ` ${lines[i++].trim()}`;
        list.append(h('li', {}, ...inline(content, base)));
      }
      root.append(list);
      continue;
    }
    const para = [];
    while (i < lines.length && lines[i].trim() && !/^(#{1,6}\s|\s*```|\s*>|\s*([-*+]|\d+[.)])\s)/.test(lines[i])
      && !(lines[i].includes('|') && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1] ?? ''))) para.push(lines[i++].trim());
    if (!para.length) {
      i++;
      continue;
    }
    root.append(h('p', {}, ...inline(para.join(' '), base)));
  }
  return root;
}

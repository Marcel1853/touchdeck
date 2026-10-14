// Beispiel-Plugin für TouchDeck. Alles, was ein Plugin darf, läuft über das Objekt „td“.

// Aktion „Begrüßen“: liefert eine Meldung zurück, die im Editor und im Protokoll erscheint.
td.action('gruessen', (params) => {
  const gruss = td.settings.gruss || 'Hallo';
  return `${gruss}, ${params.name || 'Welt'}!`;
});

// Aktion „Zähler erhöhen“: liest und setzt einen TouchDeck-Wert (Berechtigung „values“).
td.action('zaehlen', (params) => {
  const name = params.wert || 'Beispiel.Zähler';
  const next = (Number(td.values.get(name)) || 0) + 1;
  td.values.set(name, next);
  // Eigener Auslöser: Events in TouchDeck können darauf reagieren.
  if (next % 10 === 0) td.fire('zehn', { wert: name, Anzahl: next });
  return `${name} = ${next}`;
});

// Aktion „GitHub-Sterne“: Internet nur zu den Adressen aus permissions.http.
td.action('github', (params) => {
  const res = td.http({ url: `https://api.github.com/repos/${params.repo}`, headers: { 'User-Agent': 'TouchDeck-Plugin' } });
  if (!res.ok) throw new Error(`GitHub antwortet mit ${res.status}`);
  return `${params.repo}: ${res.json().stargazers_count} ⭐`;
});

// Zeitgeber: alle 60 Sekunden die Uhrzeit in einen Wert schreiben.
td.every(60000, () => {
  const now = new Date();
  td.values.set('Beispiel.Uhrzeit', `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`);
});

td.log('Beispiel-Plugin bereit');

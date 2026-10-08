/* Model: the shape of habits, check-ins and backups, and the checks that keep bad data out.
   Shared by every part (suite.json "scripts"). Pure functions only: no DOM, no state.

   A habit:   { id: "h…", name: "Read", color: "#1971c2", created: "2026-10-08" }
   Check-ins: { "<habit id>": ["2026-10-06", "2026-10-08", …] }   (sorted, no repeats)
   A backup:  { format: "wardian-habit-tracker", version: 1, exported, habits: [...], days: {...} } */
const Model = Object.freeze({
  FORMAT: 'wardian-habit-tracker',
  MAX_HABITS: 100,
  MAX_NAME: 60,
  COLORS: Object.freeze([
    ['#2f9e44', 'Green'], ['#1971c2', 'Blue'], ['#7048e8', 'Violet'], ['#d6336c', 'Pink'],
    ['#e8590c', 'Orange'], ['#0c8599', 'Teal'], ['#e0a100', 'Yellow'], ['#868e96', 'Grey'],
  ]),
  newId() { return 'h' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6); },
  colorName(hex) { const c = Model.COLORS.find(c => c[0] === hex); return c ? c[1] : hex; },

  /** Checks one habit. Returns a clean copy, or throws an Error that says what is wrong. */
  habit(h, where = 'A habit') {
    if (!h || typeof h !== 'object') throw new Error(where + ' is not an object.');
    const name = typeof h.name === 'string' ? h.name.trim() : '';
    if (!name) throw new Error(where + ' has no name.');
    if (name.length > Model.MAX_NAME) throw new Error(where + ' has a name longer than ' + Model.MAX_NAME + ' characters.');
    if (typeof h.id !== 'string' || !/^[A-Za-z0-9_-]{1,40}$/.test(h.id)) throw new Error(where + ' (' + name + ') has no valid id.');
    const color = /^#[0-9a-f]{6}$/i.test(h.color) ? h.color.toLowerCase() : Model.COLORS[0][0];
    const created = Days.valid(h.created) ? h.created : Days.today();
    return { id: h.id, name, color, created };
  },
  /** Checks a list of habits: each one, the count and that no id repeats. */
  habits(list) {
    if (!Array.isArray(list)) throw new Error('The habits are not a list.');
    if (list.length > Model.MAX_HABITS) throw new Error('There are more than ' + Model.MAX_HABITS + ' habits.');
    const out = list.map((h, i) => Model.habit(h, 'Habit ' + (i + 1)));
    const ids = new Set(out.map(h => h.id));
    if (ids.size !== out.length) throw new Error('Two habits have the same id.');
    return out;
  },
  /** Check-ins: keeps real days only, sorted and without repeats. With `ids`, keeps those habits only.
      With strict, a bad day throws instead of being dropped. */
  days(obj, ids = null, strict = false) {
    if (!obj || typeof obj !== 'object' || Array.isArray(obj)) throw new Error('The check-ins are not an object.');
    const out = {};
    for (const [id, list] of Object.entries(obj)) {
      if (ids && !ids.has(id)) { if (strict) throw new Error('Check-ins name a habit that is not in the file: ' + id + '.'); continue; }
      if (!Array.isArray(list)) { if (strict) throw new Error('The check-ins for ' + id + ' are not a list.'); continue; }
      const bad = list.find(d => !Days.valid(d));
      if (bad !== undefined && strict) throw new Error('A check-in is not a real day: ' + JSON.stringify(bad).slice(0, 40) + '.');
      out[id] = [...new Set(list.filter(Days.valid))].sort();
    }
    return out;
  },
  /** Reads a backup file's text. Returns { habits, days }, or throws an Error a person can read. */
  parseBackup(text) {
    let data;
    try { data = JSON.parse(text); } catch { throw new Error('This file is not JSON.'); }
    if (!data || data.format !== Model.FORMAT) throw new Error('This file is not a habit tracker backup.');
    if (data.version !== 1) throw new Error('This backup is version ' + data.version + '; this app reads version 1.');
    const habits = Model.habits(data.habits);
    const days = Model.days(data.days || {}, new Set(habits.map(h => h.id)), true);
    return { habits, days };
  },
  backup(habits, days) {
    const keep = Object.fromEntries(habits.map(h => [h.id, days[h.id] || []]));
    return { format: Model.FORMAT, version: 1, exported: new Date().toISOString(), habits, days: keep };
  },
  count(days) { return Object.values(days).reduce((n, list) => n + list.length, 0); },
});

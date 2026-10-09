/* Log: what every part of the focus log shares. The table, how a session from the channel is
   checked, local days, and how minutes and times read.
   The log lives in this package's own SQLite database (the `db` capability), in one table:
     sessions(id, day, label, minutes, started, ended, completed, source, received)
   `id` is the sending package plus the start time, so the same session never counts twice, even
   when Wardian replays the latest message on the channel to a log that opens later. */
const Log = (() => {
  'use strict';
  const TABLE = 'sessions';
  const COLUMNS = ['id', 'day', 'label', 'minutes', 'started', 'ended', 'completed', 'source', 'received'];
  const CREATE = 'CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, day TEXT NOT NULL, label TEXT NOT NULL, ' +
    'minutes REAL NOT NULL, started TEXT NOT NULL, ended TEXT NOT NULL, completed INTEGER NOT NULL, source TEXT NOT NULL, received TEXT NOT NULL)';

  // The database, with the table made. One promise per frame; null when the host has no database.
  let ready = null;
  function db(ctx){
    if (!ready) ready = ctx.cap('db').then(async d => {
      if (!d) return null;
      await d.query({sql: CREATE, params: []});
      return d;
    }).catch(e => { ready = null; throw e; });
    return ready;
  }

  const pad = n => String(n).padStart(2, '0');
  /** The local calendar day of a Date, as YYYY-MM-DD. */
  const day = d => d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate());
  /** The local day `n` days before today. */
  const daysAgo = n => { const d = new Date(); d.setHours(12, 0, 0, 0); d.setDate(d.getDate() - n); return day(d); };

  /* Checks one message from the channel. Returns {row} or {problem}. */
  function clean(data, from){
    if (!data || typeof data !== 'object') return {problem: 'the message is not a session'};
    const started = new Date(data.started), ended = new Date(data.ended), minutes = Number(data.minutes);
    if (isNaN(started) || isNaN(ended)) return {problem: 'the session has no valid start or end time'};
    if (!(minutes > 0 && minutes <= 24 * 60)) return {problem: 'the session\'s minutes must be between 0 and 1,440'};
    const label = String(data.label == null ? '' : data.label).trim().slice(0, 80) || 'No label';
    const source = String(from || 'unknown');
    return {row: [source + '|' + started.toISOString(), day(started), label, Math.round(minutes * 10) / 10,
      started.toISOString(), ended.toISOString(), data.completed === false ? 0 : 1, source, new Date().toISOString()]};
  }
  /* What a session is, in words: the same words Focus timer writes for what it sent. */
  const what = data => data.minutes + ' min, ' + (data.completed === false ? 'stopped early' : 'completed') + ', ended ' +
    new Date(data.ended).toLocaleTimeString(undefined, {hour: 'numeric', minute: '2-digit'});
  async function insert(d, row){
    const r = await d.query({sql: 'INSERT OR IGNORE INTO sessions (' + COLUMNS.join(', ') + ') VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)', params: row});
    return r.changed > 0;
  }

  /** 95 → "1 h 35 min"; 25 → "25 min"; 0 → "0 min". */
  function minutes(v){
    const m = Math.round(Number(v) || 0);
    return m >= 60 ? Math.floor(m / 60) + ' h' + (m % 60 ? ' ' + (m % 60) + ' min' : '') : m + ' min';
  }
  const time = iso => new Date(iso).toLocaleTimeString(undefined, {hour: 'numeric', minute: '2-digit'});
  const date = ymd => { const [y, m, d] = ymd.split('-').map(Number); return new Date(y, m - 1, d).toLocaleDateString(undefined, {weekday: 'short', day: 'numeric', month: 'short'}); };
  const errText = e => String((e && e.message) || e || 'unknown error');
  const empty = (ctx, text) => Object.assign(ctx.el('p'), {className: 'empty', textContent: text});

  return Object.freeze({TABLE, COLUMNS, db, day, daysAgo, clean, what, insert, minutes, time, date, errText, empty});
})();

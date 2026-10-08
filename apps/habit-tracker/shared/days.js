/* Days: calendar days as "YYYY-MM-DD" text, in the viewer's local time. Shared by every part
   (suite.json "scripts"). Arithmetic runs on UTC dates built from the day's numbers, so a change to
   or from summer time never skips or repeats a day. Pure functions only: no DOM, no state. */
const Days = Object.freeze({
  DAY_MS: 86400000,
  /** The local calendar day of a Date (default: now). */
  key(d = new Date()) {
    return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
  },
  today() { return Days.key(); },
  /** True for a real day such as "2026-02-28"; false for "2026-02-30" or "soon". */
  valid(key) {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(key));
    if (!m) return false;
    const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
    return d.getUTCFullYear() === +m[1] && d.getUTCMonth() === +m[2] - 1 && d.getUTCDate() === +m[3];
  },
  utc(key) { const [y, m, d] = key.split('-').map(Number); return Date.UTC(y, m - 1, d); },
  fromUtc(ms) {
    const d = new Date(ms);
    return d.getUTCFullYear() + '-' + String(d.getUTCMonth() + 1).padStart(2, '0') + '-' + String(d.getUTCDate()).padStart(2, '0');
  },
  /** "2026-10-08" plus -1 -> "2026-10-07". */
  add(key, n) { return Days.fromUtc(Days.utc(key) + n * Days.DAY_MS); },
  /** Whole days from a to b: diff("2026-10-01", "2026-10-08") -> 7. */
  diff(a, b) { return Math.round((Days.utc(b) - Days.utc(a)) / Days.DAY_MS); },
  /** 0 for Monday … 6 for Sunday. */
  weekday(key) { return (new Date(Days.utc(key)).getUTCDay() + 6) % 7; },
  /** The words for a day, e.g. "Thursday 8 October 2026" in the viewer's language. */
  label(key, opts = { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }) {
    return new Date(Days.utc(key)).toLocaleDateString(undefined, { ...opts, timeZone: 'UTC' });
  },
  /** Milliseconds until the next local midnight. */
  msToMidnight(now = new Date()) {
    return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime() - now.getTime();
  },
});

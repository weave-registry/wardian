/* Fmt: formatting shared by every app in the suite (inlined into each frame by suite.json "scripts").
   Pure functions only: no DOM, no state. */
const Fmt = Object.freeze({
  money(value, currency, digits = 0) {
    try {
      return new Intl.NumberFormat(undefined, { style: 'currency', currency, maximumFractionDigits: digits, minimumFractionDigits: digits }).format(value);
    } catch { return value.toFixed(digits); }
  },
  /** "2026-10" plus 5 months -> "Mar 2027". */
  month(start, offset) {
    const [y, m] = String(start).split('-').map(Number);
    const d = new Date(Date.UTC(y || 2026, (m || 1) - 1 + offset, 1));
    return d.toLocaleDateString(undefined, { month: 'short', year: 'numeric', timeZone: 'UTC' });
  },
  /** 279 -> "23 years 3 months". */
  span(months) {
    const y = Math.floor(months / 12), m = months % 12, parts = [];
    if (y) parts.push(y + (y === 1 ? ' year' : ' years'));
    if (m || !y) parts.push(m + (m === 1 ? ' month' : ' months'));
    return parts.join(' ');
  },
});

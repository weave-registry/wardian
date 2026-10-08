/* Fmt: numbers, durations and dates the same way in every part, and a small SVG helper. */
const Fmt = (() => {
  'use strict';
  const UNITS = {days: ['day', 'days'], weeks: ['week', 'weeks'], hours: ['hour', 'hours']};
  const num = (v, digits = 1) => Number(v).toLocaleString(undefined, {minimumFractionDigits: digits, maximumFractionDigits: digits});
  const int = v => Math.round(Number(v)).toLocaleString();
  const unitWord = (unit, v) => (UNITS[unit] || UNITS.days)[Math.abs(v - 1) < 1e-9 ? 0 : 1];
  const dur = (v, unit, digits = 1) => num(v, digits) + ' ' + unitWord(unit, Number(num(v, digits).replace(/[^\d.-]/g, '')));

  // The calendar date a duration lands on, counted from `start` (YYYY-MM-DD). Days and weeks are
  // calendar days: weekends and holidays count. Hours have no date.
  function date(start, v, unit, iso){
    if (!start || unit === 'hours' || !/^\d{4}-\d{2}-\d{2}$/.test(start)) return '';
    const days = Math.max(0, Math.ceil(unit === "weeks" ? v * 7 : v) - 1);       // a 1-day task ends on the start day
    const [y, m, d] = start.split('-').map(Number);
    const at = new Date(y, m - 1, d + days);
    if (iso) return at.getFullYear() + '-' + String(at.getMonth() + 1).padStart(2, '0') + '-' + String(at.getDate()).padStart(2, '0');
    return at.toLocaleDateString(undefined, {weekday: 'short', day: 'numeric', month: 'short', year: 'numeric'});
  }
  const pct = (v, digits = 0) => (v * 100).toFixed(digits) + '%';
  const seconds = ms => ms < 1000 ? Math.max(1, Math.round(ms)) + ' ms' : (ms / 1000).toFixed(1) + ' s';
  const errText = e => String((e && e.message) || e || 'unknown error');

  const NS = 'http://www.w3.org/2000/svg';
  function svg(tag, attrs, text){
    const n = document.createElementNS(NS, tag);
    for (const [k, v] of Object.entries(attrs || {})) n.setAttribute(k, v);
    if (text !== undefined) n.textContent = text;
    return n;
  }
  return Object.freeze({num, int, dur, date, pct, seconds, errText, svg, unitWord});
})();

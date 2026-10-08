/* Stats: streaks and completion for one habit. Shared by every part (suite.json "scripts").
   Pure functions only: no DOM, no state. */
const Stats = Object.freeze({
  /** `list` is the habit's sorted check-in days. Today still counts as open: a streak that ended
      yesterday is still current until today is over. */
  of(list, today, created) {
    const set = new Set(list);
    let day = set.has(today) ? today : Days.add(today, -1), current = 0;
    while (set.has(day)) { current++; day = Days.add(day, -1); }

    let longest = 0, run = 0, prev = null;
    for (const d of list) {
      if (d > today) continue;
      run = prev && Days.diff(prev, d) === 1 ? run + 1 : 1;
      longest = Math.max(longest, run);
      prev = d;
    }
    // A habit counts from the day it was made, or from its first check-in if that is earlier.
    const first = list.length && list[0] < created ? list[0] : created;
    const rate = n => {
      const start = [Days.add(today, -(n - 1)), first].sort()[1];
      const days = Days.diff(start, today) + 1;
      if (days <= 0) return { done: 0, days: 0, pct: null };
      const done = list.filter(d => d >= start && d <= today).length;
      return { done, days, pct: done / days };
    };
    return { current, longest, total: list.filter(d => d <= today).length, doneToday: set.has(today), d30: rate(30), d90: rate(90) };
  },
  pct(r) { return r.pct === null ? '–' : Math.round(r.pct * 100) + '%'; },
  plural(n, one, many = one + 's') { return n + ' ' + (n === 1 ? one : many); },
});

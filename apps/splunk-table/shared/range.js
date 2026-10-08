/* TimeRange: reads the time range a person chose, shared by the parts that run searches.
   Ready-made options hold "earliest|latest" in Splunk time codes; "rel", "abs" and "adv" read the
   extra fields. Pure: it is given the field values, never the page. */
const TimeRange = Object.freeze({
  UNITS: {m: ['minute', 'minutes'], h: ['hour', 'hours'], d: ['day', 'days'], w: ['week', 'weeks'], mon: ['month', 'months']},
  CODE: /^[A-Za-z0-9@+\-:\/._]{1,40}$/,     // what a Splunk time code may look like
  KINDS: ['rel', 'abs', 'adv'],
  fmt: t => new Date(t).toLocaleString([], {dateStyle: 'medium', timeStyle: 'short'}),
  // A Date as the value of an <input type="datetime-local">.
  localInput(d){
    const pad = n => String(n).padStart(2, '0');
    return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + 'T' + pad(d.getHours()) + ':' + pad(d.getMinutes());
  },

  // v: {range, label (of the chosen option), relN, relUnit, absFrom, absTo, advE, advL}.
  // Returns {earliest, latest, label}, or {error} with a short message.
  read(v){
    const U = TimeRange.UNITS;
    if (v.range === 'rel'){
      const n = Number(v.relN), u = v.relUnit;
      if (!Number.isInteger(n) || n < 1 || n > 100000 || !U[u]) return {error: 'Type a whole number of 1 or more under How many.'};
      return {earliest: '-' + n + u, latest: '', label: 'Last ' + n + ' ' + U[u][n === 1 ? 0 : 1]};
    }
    if (v.range === 'abs'){
      const a = v.absFrom, b = v.absTo;
      const ta = a ? new Date(a).getTime() : NaN, tb = b ? new Date(b).getTime() : Date.now();
      if (!a || isNaN(ta)) return {error: 'Choose the date and time to start from.'};
      if (isNaN(tb)) return {error: 'The end date is not valid.'};
      if (tb <= ta) return {error: 'The end must be after the start.'};
      return {earliest: String(Math.floor(ta / 1000)), latest: b ? String(Math.floor(tb / 1000)) : '',
        label: TimeRange.fmt(ta) + ' to ' + (b ? TimeRange.fmt(tb) : 'now')};
    }
    if (v.range === 'adv'){
      const e = String(v.advE || '').trim(), l = String(v.advL || '').trim();
      if ((e && !TimeRange.CODE.test(e)) || (l && !TimeRange.CODE.test(l))) return {error: 'Splunk time codes look like -2d@d, @w1, -90m or now.'};
      return {earliest: e, latest: l, label: 'Splunk time ' + (e || 'all time') + ' to ' + (l || 'now')};
    }
    const [e = '', l = ''] = String(v.range || '').split('|');
    return {earliest: e, latest: l, label: v.label || ''};
  },
});

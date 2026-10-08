/* Bars: draws counts as an SVG bar chart. Used only by the chart part (its suite.json "scripts").
   Ordered groups (number ranges, years, months) stand as columns left to right; text values lie as
   rows, the largest first. One colour, thin recessive grid, 4px rounded ends, a 2px gap between bars.
   Each bar answers hover and focus through onPick(index or -1). */
const Bars = Object.freeze({
  NS: 'http://www.w3.org/2000/svg',
  svg(tag, attrs, text){
    const e = document.createElementNS(Bars.NS, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    if (text != null) e.textContent = text;
    return e;
  },
  // A step of 1, 2 or 5 times a power of ten, near span/count.
  step(span, count){
    const raw = span / Math.max(1, count), p = Math.pow(10, Math.floor(Math.log10(raw || 1))), f = raw / p;
    return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * p;
  },
  // A bar rectangle whose far end has rounded corners: up for columns, right for rows.
  path(x, y, w, h, up){
    const r = Math.min(4, w / 2, h / 2);
    if (r <= 0) return '';
    return up
      ? `M${x},${y + h}V${y + r}Q${x},${y} ${x + r},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h}Z`
      : `M${x},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h - r}Q${x + w},${y + h} ${x + w - r},${y + h}H${x}Z`;
  },
  // groups: [{label, short, n}]. Returns the <svg> element.
  draw(groups, {width, columns, onPick}){
    const max = Math.max(1, ...groups.map(g => g.n)), ticks = Bars.step(max, 4);
    const top = Math.ceil(max / ticks) * ticks, fmt = n => Number(n).toLocaleString();
    let s, bars = [];
    if (columns){
      const H = 260, L = 8 + 7 * fmt(top).length, B = 34, T = 8, R = 8, plotW = width - L - R, plotH = H - T - B;
      s = Bars.svg('svg', {viewBox: `0 0 ${width} ${H}`, width, height: H, role: 'list'});
      for (let v = 0; v <= top; v += ticks){
        const y = T + plotH - v / top * plotH;
        s.append(Bars.svg('line', {x1: L, x2: width - R, y1: y, y2: y, class: v ? 'grid' : 'axis'}),
          Bars.svg('text', {x: L - 6, y: y + 4, 'text-anchor': 'end', class: 'tick'}, fmt(v)));
      }
      const slot = plotW / groups.length, w = Math.max(1, slot - 2), every = Math.ceil(groups.length / Math.max(1, Math.floor(plotW / 64)));
      groups.forEach((g, i) => {
        const x = L + i * slot + 1, h = g.n / top * plotH;
        bars.push(Bars.svg('path', {d: Bars.path(x, T + plotH - h, w, h, true), class: 'bar'}));
        if (i % every === 0) s.append(Bars.svg('text', {x: L + i * slot + (g.edge ? 0 : slot / 2), y: H - B + 18, 'text-anchor': g.edge ? 'start' : 'middle', class: 'tick'}, g.short));
      });
      bars.forEach((b, i) => s.append(Bars.hit(b, L + i * slot, T, slot, plotH, i, groups[i], onPick)));
    } else {
      const row = 26, L = Math.min(180, 12 + 7 * Math.max(...groups.map(g => g.short.length))), R = 64;
      const H = groups.length * row + 8, plotW = width - L - R;
      s = Bars.svg('svg', {viewBox: `0 0 ${width} ${H}`, width, height: H, role: 'list'});
      s.append(Bars.svg('line', {x1: L, x2: L, y1: 0, y2: H - 4, class: 'axis'}));
      groups.forEach((g, i) => {
        const y = 4 + i * row, w = g.n / max * plotW;
        s.append(Bars.svg('text', {x: L - 8, y: y + row / 2 + 4, 'text-anchor': 'end', class: 'label'}, g.short),
          Bars.svg('text', {x: L + w + 6, y: y + row / 2 + 4, class: 'tick'}, fmt(g.n)));
        bars.push(Bars.svg('path', {d: Bars.path(L, y + 3, w, row - 6, false), class: 'bar'}));
      });
      bars.forEach((b, i) => s.append(Bars.hit(b, 0, 4 + i * row, width, row, i, groups[i], onPick)));
    }
    return s;
  },
  // A bar inside a hit area larger than the bar, focusable, with a title for screen readers.
  hit(bar, x, y, w, h, i, g, onPick){
    const grp = Bars.svg('g', {tabindex: 0, role: 'listitem', 'aria-label': g.label + ': ' + g.n.toLocaleString()});
    grp.append(Bars.svg('rect', {x, y, width: Math.max(1, w), height: h, class: 'hit'}), bar, Bars.svg('title', {}, g.label + ': ' + g.n.toLocaleString()));
    const on = () => { grp.classList.add('on'); onPick(i); }, off = () => { grp.classList.remove('on'); onPick(-1); };
    grp.addEventListener('pointerenter', on); grp.addEventListener('pointerleave', off);
    grp.addEventListener('focus', on); grp.addEventListener('blur', off);
    return grp;
  },
});

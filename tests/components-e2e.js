// The component library's glass, tiles and receipt keep the promises of docs/site/components.md.
// Run with tests/run-components-e2e.sh: BASE=http://127.0.0.1:PORT node tests/components-e2e.js
const { chromium } = require('playwright');
const B = process.env.BASE;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };

// In the page: the lowest contrast of `textSel`'s colour on the glass `paneSel`, with each of the
// wallpaper's lights at full strength behind it, and with none. The blur's extra saturation is left out.
const lowestContrast = ({ paneSel, textSel }) => {
  const cv = document.createElement('canvas'); cv.width = cv.height = 1;
  const cx = cv.getContext('2d', { willReadFrequently: true });
  const rgba = c => { cx.clearRect(0, 0, 1, 1); cx.fillStyle = '#000'; cx.fillStyle = c; cx.fillRect(0, 0, 1, 1); const d = cx.getImageData(0, 0, 1, 1).data; return [d[0], d[1], d[2], d[3] / 255]; };
  const over = (top, under) => top.slice(0, 3).map((v, i) => v * top[3] + under[i] * (1 - top[3]));
  const lum = c => { const f = v => { v /= 255; return v <= .03928 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4; }; return .2126 * f(c[0]) + .7152 * f(c[1]) + .0722 * f(c[2]); };
  const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + .05) / (y + .05); };
  const pane = document.querySelector(paneSel), wall = pane.closest('.w-wallpaper');
  const ws = getComputedStyle(wall), base = rgba(ws.backgroundColor);
  const glass = rgba(getComputedStyle(pane).backgroundColor), text = rgba(getComputedStyle(pane.querySelector(textSel)).color);
  const behind = [base, ...['--w-glow-1', '--w-glow-2', '--w-glow-3'].map(g => over(rgba(ws.getPropertyValue(g).trim()), base))];
  return Math.min(...behind.map(b => ratio(over(text, over(glass, b)), over(glass, b))));
};

(async () => {
  const browser = await chromium.launch(require('./browser')());
  const errors = [];

  for (const scheme of ['light', 'dark']) {
    const p = await browser.newPage({ viewport: { width: 1200, height: 900 }, colorScheme: scheme });
    p.on('pageerror', e => errors.push(e.message));
    await p.goto(B + '/ui/');
    // components.md, Glass: quiet text on glass meets 4.5:1 whatever light is behind it.
    for (const [sel, what] of [['.w-card-description', 'quiet text'], ['.w-card-title', 'text']]) {
      const c = await p.evaluate(lowestContrast, { paneSel: '#surface .w-glass', textSel: sel });
      ok(c >= 4.5, `${scheme}: ${what} on glass is at least 4.5:1 over every light (${c.toFixed(2)})`);
    }
    await p.close();
  }

  const p = await browser.newPage({ viewport: { width: 1200, height: 900 } });
  p.on('pageerror', e => errors.push(e.message));
  await p.goto(B + '/ui/');
  const glass = () => p.evaluate(() => { const s = getComputedStyle(document.querySelector('#surface .w-glass')); return { bg: s.backgroundColor, filter: s.backdropFilter }; });
  const before = await glass();
  ok(before.filter.includes('blur') && !/^rgb\(/.test(before.bg), `glass blurs and lets light through (${before.filter}, ${before.bg})`);

  // components.md: a viewer who asks for less transparency gets solid panes.
  const cdp = await p.context().newCDPSession(p);
  await cdp.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-transparency', value: 'reduce' }] });
  const solid = await glass();
  ok(solid.filter === 'none' && /^rgb\(/.test(solid.bg), `less transparency: the pane is solid (${solid.filter}, ${solid.bg})`);
  await cdp.send('Emulation.setEmulatedMedia', { features: [] });

  // components.md: a viewer who asks for less motion gets a still light.
  const anim = () => p.evaluate(() => getComputedStyle(document.querySelector('#surface .w-wallpaper'), '::before').animationName);
  const moving = await anim();
  await p.emulateMedia({ reducedMotion: 'reduce' });
  const still = await anim();
  ok(moving === 'w-drift' && still === 'none', `less motion: the light stops (${moving} → ${still})`);
  await p.emulateMedia({ reducedMotion: 'no-preference' });

  // components.md, bento: tiles of different widths; headers, bodies and footers line up along a row.
  const tiles = () => p.evaluate(() => [...document.querySelectorAll('#bento .w-tile')].map(t => {
    const r = t.getBoundingClientRect(), b = t.querySelector('.w-tile-body').getBoundingClientRect();
    return { w: Math.round(r.width), top: Math.round(r.top), body: Math.round(b.top) };
  }));
  const wide = await tiles();
  ok(wide[0].w > wide[2].w * 1.8, `a span-8 tile is twice as wide as a span-4 tile (${wide[0].w} and ${wide[2].w})`);
  ok(wide[0].top === wide[1].top && wide[0].body === wide[1].body, `tiles in one row start their bodies at the same height (${wide[0].body} and ${wide[1].body})`);
  // The grid reads its own width, not the screen's: in a 420px box on a 1200px screen, tiles stack.
  await p.evaluate(() => { document.querySelector('#bento .w-bento').style.width = '420px'; });
  const bentoW = await p.evaluate(() => Math.round(document.querySelector('#bento .w-bento').getBoundingClientRect().width));
  const narrow = await tiles();
  ok(narrow.every(t => Math.abs(t.w - bentoW) <= 1), `in a narrow box every tile takes the whole width (${narrow.map(t => t.w).join(', ')} of ${bentoW})`);

  // components.md, receipt: the same message looks the same in both apps; the id gives the colour and the short id.
  const r = await p.evaluate(() => {
    const id = '3f9a2c1e-7b4d-4c2a-9e51-0d6f8a3b2c47', at = Date.UTC(2026, 9, 9, 14, 41);
    const data = { fields: ['host', 'requests', 'p95', 'errors', 'region'], rows: [['a', 1, 2, 0, 'eu']], dataset: { total: 1000 } };
    const sent = WardianUI.receipt({ id, name: '<b>Checkout</b>', channel: 'splunk.table', at, data }, { direction: 'sent' });
    const got = WardianUI.receipt({ id, name: '<b>Checkout</b>', from: 'splunk-table', channel: 'splunk.table', at, data }, { direction: 'received' });
    const other = WardianUI.receipt({ id: '0a1b2c3d-0000-4000-8000-000000000000' });
    const hue = e => e.style.getPropertyValue('--w-receipt-hue');
    const shown = document.querySelector('#receipt .w-receipt');
    return { hues: [hue(sent), hue(got), hue(other)], ids: [sent, got].map(e => e.querySelector('.w-receipt-id').textContent),
      tags: sent.querySelectorAll('b').length, name: sent.querySelector('.w-receipt-name').textContent,
      metas: [sent, got].map(e => e.querySelector('.w-receipt-meta').textContent), whats: [sent, got].map(e => e.querySelector('.w-receipt-what').textContent),
      kinds: [WardianUI.describe([1, 2, 3]), WardianUI.describe({ label: 'Write', minutes: 25 }), WardianUI.describe('abc'), WardianUI.describe({ fields: ['a'], rows: [[1]] })],
      gallery: hue(shown), short: WardianUI.shortId(id) };
  });
  ok(r.hues[0] === r.hues[1] && r.hues[0] !== r.hues[2], `the same id gives the same colour, another id another (${r.hues.join(', ')})`);
  ok(r.ids.every(x => x === '#3f9a2c') && r.short === '#3f9a2c', `both show the short id #3f9a2c (${r.ids.join(', ')})`);
  ok(r.tags === 0 && r.name === '<b>Checkout</b>', 'a name is shown as text, never as markup');
  ok(/^Sent to other apps on splunk\.table at .+, \d+ bytes#3f9a2c$/.test(r.metas[0]) && /^Received from splunk-table on splunk\.table at .+, \d+ bytes#3f9a2c$/.test(r.metas[1]),
    `the sender says Sent and where, the receiver Received and from which app (${r.metas.join(' | ')})`);
  ok(r.whats.every(w => w === '1,000 rows, 5 columns: host, requests, p95, errors and 1 more'), `both say what the data holds, counting rows left in the database (${r.whats[0]})`);
  ok(r.kinds.join(' | ') === '3 items | Fields: label, minutes | Text, 3 characters | 1 row, 1 column: a', `other kinds of data are described too (${r.kinds.join(' | ')})`);
  ok(r.gallery.trim() === r.hues[0], `the gallery's receipt has the colour receipt.js gives its id (${r.gallery} and ${r.hues[0]})`);

  ok(errors.length === 0, 'no page errors' + (errors.length ? ': ' + errors.join(' | ') : ''));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });

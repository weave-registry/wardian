// End-to-end test of Save as web page (ADR-2610080905). Run by tests/run-snapshot-e2e.sh, which
// starts a private Wardian with the loan planner, Mandelbrot, adder and a suite made to attack it.
const { chromium } = require('playwright');
const fs = require('fs'), path = require('path');
const B = process.env.BASE, OUT = process.env.OUT;
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const POLICY = "default-src 'none'; style-src 'unsafe-inline'; img-src data:";
const frameOf = (page, end) => page.frames().find(f => f.url().split('?')[0].endsWith(end));

async function open(page, name) {
  await page.locator('#apps li button', { hasText: name }).first().click();
  await page.locator('#saveWebBtn').waitFor({ timeout: 5000 });
}

// Wait until every app of the open suite has started, so each can answer with its own snapshot.
async function started(page, name, n) {
  for (let i = 0; i < 600; i++) {               // up to a minute, for a slow machine
    const k = frameOf(page, '/run/' + name + '/');
    if (k && await k.evaluate(() => window.Kernel ? Kernel.started().length : 0).catch(() => 0) >= n) return k;
    await sleep(100);
  }
  throw new Error(name + ' did not start');
}

async function save(page, name, wait = 20000) {
  await page.click('#saveWebBtn');
  const warn = await page.locator('#saveWebBar').textContent();
  const [dl] = await Promise.all([page.waitForEvent('download', { timeout: wait }), page.click('#saveWebGo')]);
  const file = path.join(OUT, dl.suggestedFilename());
  await dl.saveAs(file);
  ok(new RegExp('^' + name + '-\\d{4}-\\d{2}-\\d{2}\\.html$').test(dl.suggestedFilename()), 'saved as ' + dl.suggestedFilename());
  return { file, html: fs.readFileSync(file, 'utf8'), warn };
}

// Read the saved file as an inert document in the browser, and report what is in it.
const inspect = (page, html) => page.evaluate(h => {
  const d = new DOMParser().parseFromString(h, 'text/html');
  const all = [...d.querySelectorAll('*')];
  const urls = [];
  for (const e of all) for (const a of e.attributes) {
    const n = a.name.toLowerCase();
    if (['src', 'href', 'xlink:href', 'srcset', 'action', 'formaction', 'poster', 'data'].includes(n) && !/^(data:image\/|#)/i.test(a.value.trim())) urls.push(e.localName + ' ' + n + '=' + a.value.slice(0, 60));
  }
  const css = [...d.querySelectorAll('style')].map(s => s.textContent).join('\n') + all.map(e => e.getAttribute('style') || '').join('\n');
  return {
    first: (d.head.firstElementChild && d.head.firstElementChild.outerHTML) || '',
    csp: (d.querySelector('meta[http-equiv="Content-Security-Policy"]') || {}).content || '',
    bad: all.filter(e => ['script', 'iframe', 'frame', 'object', 'embed', 'link', 'base', 'form', 'foreignobject'].includes(e.localName.toLowerCase())).map(e => e.localName),
    metas: all.filter(e => e.localName === 'meta' && !['Content-Security-Policy'].includes(e.getAttribute('http-equiv')) && !e.hasAttribute('charset') && e.getAttribute('name') !== 'viewport').length,
    on: all.flatMap(e => [...e.attributes].filter(a => /^on/i.test(a.name)).map(a => e.localName + ' ' + a.name)),
    js: /javascript:/i.test(h),
    urls,
    cssUrls: (css.match(/url\s*\(\s*['"]?(?!data:image\/)[^)]*\)/gi) || []).concat(css.match(/@import/gi) || []),
    parts: [...d.querySelectorAll('[data-part]')].map(e => e.getAttribute('data-part')),
    text: d.body.textContent.replace(/\s+/g, ' '),
  };
}, html);

function safe(r, what) {
  ok(r.first.includes('Content-Security-Policy') && r.csp === POLICY, what + ': the file starts with the policy');
  ok(r.bad.length === 0 && r.metas === 0, what + ': no script, frame, object, embed, link, form or other meta ' + JSON.stringify(r.bad));
  ok(r.on.length === 0, what + ': no on… attribute ' + JSON.stringify(r.on));
  ok(!r.js && r.urls.length === 0 && r.cssUrls.length === 0, what + ': no javascript: and no outside address ' + JSON.stringify(r.urls.concat(r.cssUrls)).slice(0, 300));
  ok(/A copy that does not update/.test(r.text), what + ': the header says it is a copy that does not update');
}

// Open the saved file from disk with every request blocked, and count what it tries to load.
async function offline(browser, file, check) {
  const ctx = await browser.newContext({ offline: true });
  const tried = [];
  await ctx.route('**/*', r => { const u = r.request().url(); if (u === 'file://' + file) return r.continue(); tried.push(u); return r.abort(); });
  const p = await ctx.newPage();
  p.on('request', r => { if (r.url() !== 'file://' + file) tried.push(r.url()); });
  await p.goto('file://' + file); await sleep(500);
  const extra = check ? await check(p) : true;
  await ctx.close();
  return { tried: [...new Set(tried)], extra };
}

(async () => {
  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const ctx = await browser.newContext({ acceptDownloads: true, viewport: { width: 1360, height: 1000 } });
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto(B + '/'); await sleep(800);

  console.log('== a suite: the loan planner, with a panel hidden by Arrange');
  await open(page, /Loan planner/i);
  const kernel = await started(page, 'loan-planner', 6);
  await sleep(500);
  const inputs = frameOf(page, '/frame/loan-planner/inputs');
  await inputs.locator('#amount').fill('250000');
  await inputs.locator('#amount').dispatchEvent('input');
  await sleep(800);
  const tiles = (await frameOf(page, '/frame/loan-planner/summary').locator('#tiles').textContent()).replace(/\s+/g, ' ').trim();
  await page.locator('#arrangeTool').waitFor({ state: 'visible', timeout: 8000 });
  await page.click('#arrangeTool');
  await kernel.locator('.w-arrange-bar').waitFor({ state: 'visible', timeout: 5000 });
  await sleep(300);
  await kernel.locator('[data-arrange-panel=export] > .w-arrange-cover button[data-a=hide]').click();
  await kernel.locator('[data-arrange-panel=export]').waitFor({ state: 'hidden', timeout: 5000 }).catch(() => {});
  await page.click('#arrangeTool'); await sleep(300);
  ok(await kernel.locator('[data-arrange-panel=export]').isHidden(), 'Arrange hid the export panel');
  const loan = await save(page, 'loan-planner');
  ok(/Everything on screen goes into the file/.test(loan.warn), 'the button warns before saving: ' + loan.warn.slice(0, 80));
  const lr = await inspect(page, loan.html);
  safe(lr, 'loan planner');
  ok(lr.parts.join() === 'header,inputs,summary,chart', 'the visible panels, in the viewer\'s order, and not the hidden one: ' + lr.parts.join());
  ok(!/Download schedule/.test(lr.text), 'the hidden export panel\'s text is not in the file');
  const firstTile = tiles.slice(0, 30);
  ok(tiles.length > 10 && lr.text.includes(firstTile), 'the summary shows what was on screen: ' + firstTile);
  ok(await page.evaluate(h => { const d = new DOMParser().parseFromString(h, 'text/html'); return d.querySelector('[data-part=inputs] #amount').getAttribute('value'); }, loan.html) === '250000', 'the amount typed is written into the file');
  ok(await page.evaluate(h => new DOMParser().parseFromString(h, 'text/html').querySelectorAll('[data-part=chart] img[src^="data:image/svg+xml"]').length, loan.html) >= 1, 'the loan chart is a data: image');
  const lo = await offline(browser, loan.file, p => p.evaluate(() => {
    const img = document.querySelector('[data-part=chart] img');
    return { text: document.body.innerText.includes('Balance over time'), img: !!img && img.complete && img.naturalWidth > 0 };
  }));
  ok(lo.tried.length === 0, 'opened from disk with the network off, it makes no request ' + JSON.stringify(lo.tried));
  ok(lo.extra.text && lo.extra.img, 'and it shows the panels and the chart ' + JSON.stringify(lo.extra));

  console.log('== a page app: Mandelbrot, with a panel hidden by Arrange');
  await open(page, /Mandelbrot/i);
  await page.locator('#arrangeTool').waitFor({ state: 'visible', timeout: 8000 });
  await sleep(1500);
  const mandel = frameOf(page, '/apps/mandelbrot/index.html');
  await page.click('#arrangeTool');
  await mandel.locator('.w-arrange-bar').waitFor({ state: 'visible', timeout: 5000 });
  await mandel.locator('[data-arrange-panel=help] > .w-arrange-cover button[data-a=hide]').dispatchEvent('click');
  await mandel.locator('[data-arrange-panel=help]').waitFor({ state: 'hidden', timeout: 5000 }).catch(() => {});
  await page.click('#arrangeTool'); await sleep(300);
  const mb = await save(page, 'mandelbrot');
  const mr = await inspect(page, mb.html);
  safe(mr, 'Mandelbrot');
  ok(/Where you are/.test(mr.text) && /centre/.test(mr.text) && /Controls/.test(mr.text), 'the page\'s panels are in the file');
  ok(!/How to explore/.test(mr.text), 'the panel hidden with Arrange is not');
  ok(await page.evaluate(h => new DOMParser().parseFromString(h, 'text/html').querySelectorAll('img[src^="data:image/png"]').length, mb.html) === 1, 'the canvas is a data: image');
  ok(/--w-font/.test(mb.html), 'the stylesheets the page links to are inlined');
  const mo = await offline(browser, mb.file, p => p.evaluate(() => { const i = document.querySelector('img'); return !!i && i.naturalWidth > 0; }));
  ok(mo.tried.length === 0 && mo.extra, 'opened from disk it makes no request and shows the picture ' + JSON.stringify(mo.tried));

  console.log('== a module app: adder, drawn by Wardian');
  await open(page, /adder/i);
  await page.locator('.fn-card').first().waitFor({ timeout: 5000 });
  const card = page.locator('.fn-card').first();
  await card.locator('.w-input').first().fill('2');
  if (await card.locator('.w-input').count() > 1) await card.locator('.w-input').nth(1).fill('3');
  await card.locator('.w-button').click();
  const result = (await card.locator('output').textContent()).trim();
  const ad = await save(page, 'adder');
  const ar = await inspect(page, ad.html);
  safe(ar, 'adder');
  ok(result.length > 0 && ar.text.includes(result), 'the card\'s result is in the file: ' + result);
  ok(await page.evaluate(h => new DOMParser().parseFromString(h, 'text/html').querySelector('.fn-card .w-input').getAttribute('value'), ad.html) === '2', 'and what was typed');
  ok(!/Arrange/.test(ar.text), 'Arrange\'s own controls are not');
  const ao = await offline(browser, ad.file);
  ok(ao.tried.length === 0, 'opened from disk it makes no request ' + JSON.stringify(ao.tried));

  console.log('== a suite that tries to put code in the file');
  await open(page, /Snapshot test/i);
  await started(page, 'snap-test', 3);
  const plain = frameOf(page, '/frame/snap-test/plain');
  await plain.locator('#t').fill('hello snap');
  await plain.locator('#s').selectOption('two');
  await plain.locator('#c').dispatchEvent('click');
  await plain.locator('#n').fill('a note');
  const t0 = Date.now();
  const ev = await save(page, 'snap-test', 25000);
  ok(Date.now() - t0 < 16000, 'a part that never answers is given up on after 10 seconds (' + (Date.now() - t0) + ' ms)');
  const er = await inspect(page, ev.html);
  safe(er, 'the attacking suite');
  ok(!/<script/i.test(ev.html) && !/onerror/i.test(ev.html) && !/foreignObject/i.test(ev.html), 'neither the script nor the onerror nor foreignObject reached the file');
  ok(/rendered by snapshot/.test(er.text) && !/shown on screen/.test(er.text), 'the part\'s own snapshot is used in place of the copy');
  ok(/This part could not be saved: slow/.test(er.text), 'the part that did not answer is named');
  const details = await page.evaluate(h => {
    const d = new DOMParser().parseFromString(h, 'text/html');
    return {
      t: d.querySelector('#t').getAttribute('value'), s: d.querySelector('#s option:checked') && d.querySelector('#s option[selected]').textContent,
      c: d.querySelector('#c').hasAttribute('checked'), n: d.querySelector('#n').textContent,
      bad: !!d.querySelector('#bad[href]'), good: d.querySelector('#good').getAttribute('href'),
      styled: d.querySelector('#styled').getAttribute('style'), dot: !!d.querySelector('img[alt=dot][src^="data:"]'),
      svg: !!d.querySelector('svg circle'), kept: d.querySelector('input[name=q]') && d.querySelector('input[name=q]').getAttribute('value'),
    };
  }, ev.html);
  ok(details.t === 'hello snap' && details.s === 'two' && details.c && details.n === 'a note', 'typed text, the chosen option, the ticked box and the notes are in the file ' + JSON.stringify(details));
  ok(!details.bad && details.good === '#evil-text', 'the javascript: link lost its address; the #link kept it');
  ok(/color:\s*red/.test(details.styled) && !/example\.com/.test(details.styled), 'a style keeps its colour and loses its outside url(): ' + details.styled);
  ok(details.dot && details.svg && details.kept === 'kept', 'safe parts stay: a data: image, the SVG, the form\'s field');
  ok(!/fonts\.googleapis|@import|example\.com/.test(ev.html), 'no web font, @import or outside address anywhere');
  ok(/rgb\(1, 2, 3\)|rgb\(1,2,3\)/.test(ev.html), 'the suite\'s own CSS is in the file');
  // ADR-2610080905 / security.md: the saved file holds no script and reaches nothing outside.
  for (const [re, what] of [[/vbscript:/i, 'vbscript:'], [/srcset/i, 'srcset'], [/xlink:href/i, 'xlink:href'], [/<animate/i, '<animate>'],
    [/<set\b/i, '<set>'], [/@font-face/i, '@font-face'], [/expression\s*\(/i, 'expression('], [/u\\72l\s*\(/i, 'an escaped u\\72l(']])
    ok(!re.test(ev.html), `the cleaner removes ${what}`);
  ok(/escaped/.test(er.text) && !!(await page.evaluate(h => new DOMParser().parseFromString(h, 'text/html').querySelector('#vb'), ev.html)), 'the elements around them stay');
  const eo = await offline(browser, ev.file, p => p.evaluate(() => window.ran === undefined));
  ok(eo.tried.length === 0 && eo.extra, 'opened from disk it runs nothing and makes no request ' + JSON.stringify(eo.tried));

  console.log('== too big');
  await page.evaluate(() => { window.appRendering = async () => ({ kind: 'page', parts: [{ name: 'small', html: '<p>hi</p>' }, { name: 'huge', html: '<p>' + 'x'.repeat(26 * 1024 * 1024) + '</p>' }], css: '' }); });
  await page.click('#saveWebGo');
  await page.locator('#saveWebMsg.err').waitFor({ timeout: 10000 });
  const big = await page.locator('#saveWebMsg').textContent();
  ok(/over the 25 MB limit/.test(big) && /largest part is huge/.test(big), 'a file over 25 MB is refused, naming the largest part: ' + big.slice(0, 120));

  ok(errors.length === 0, 'no page errors ' + JSON.stringify(errors));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });

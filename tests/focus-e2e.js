// A share between two example apps, as the user sees it: Focus timer sends a session, Focus log
// receives it, and both show the same receipt: Sent or Received, the same name, id and summary
// (ADR-2610091338). Usage: node tests/focus-e2e.js http://127.0.0.1:PORT
const { chromium } = require('playwright');
const base = process.argv[2];
let failed = 0;
const ok = (cond, what, extra = '') => { console.log(`  ${cond ? 'ok  ' : 'FAIL'} ${what} ${extra}`); if (!cond) failed++; };

(async () => {
  const browser = await chromium.launch(require('./browser')());
  const ctx = await browser.newContext();
  const errors = [];
  const open = async (title) => {
    const p = await ctx.newPage();
    p.on('pageerror', e => errors.push(e.message));
    await p.goto(base);
    await p.click(`#apps button:has-text("${title}")`);
    return p;
  };

  // Focus log asks to read focus.session.
  const logTab = await open('Focus log');
  const kernel = logTab.frameLocator('.suiteframe');
  await kernel.locator('.wardian-perm').waitFor();
  await kernel.locator('.wardian-perm button.yes').click();
  const inbox = kernel.frameLocator('iframe[title="inbox"]');
  await inbox.locator('#state:has-text("Receiving")').waitFor({ timeout: 5000 });

  // Focus timer sends a test session and asks to send.
  const timerTab = await open('Focus timer');
  const timer = timerTab.frameLocator('.appframe');
  await timer.locator('#label').fill('Write the report');
  await timer.locator('#test').dispatchEvent('click');
  await timerTab.locator('.wardian-perm').waitFor();
  await timerTab.locator('.wardian-perm button.yes').click();

  const receiptOf = async box => {
    await box.locator('.w-receipt').first().waitFor({ timeout: 5000 });
    return Object.fromEntries(await Promise.all(['name', 'dir', 'id', 'what', 'meta'].map(async p => [p, (await box.locator('.w-receipt-' + p).first().textContent()).trim()])));
  };
  const sent = await receiptOf(timer.locator('#sent'));
  const got = await receiptOf(inbox.locator('#last'));
  ok(sent.dir === 'Sent' && /to other apps on focus\.session/.test(sent.meta), 'Focus timer says Sent, and on which channel', `(${sent.meta})`);
  ok(got.dir === 'Received' && /from focus-timer on focus\.session/.test(got.meta), 'Focus log says Received from focus-timer', `(${got.meta})`);
  ok(/^Write the report, \d+(\.\d+)? min$/.test(sent.name) && got.name === sent.name, 'both name the session after its label', `(${sent.name} | ${got.name})`);
  ok(/^#[0-9a-f]{6}$/.test(sent.id) && got.id === sent.id, 'both show the same id', `(${sent.id} | ${got.id})`);
  ok(/ min, completed, ended /.test(sent.what) && got.what === sent.what, 'both say what the session was', `(${sent.what} | ${got.what})`);

  ok(errors.length === 0, 'no page errors', errors.join(' | '));
  await browser.close();
  console.log(failed ? `\n${failed} failed` : '\nall passed');
  process.exit(failed ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });

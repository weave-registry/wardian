// Channels between separate apps: a page app sends, a suite in another tab receives,
// and the user's permission is asked, remembered, revoked and refused.
// Usage: node tests/channels-e2e.js http://127.0.0.1:PORT   (apps folder must hold chan-sender and chan-viewer)
const { chromium } = require('playwright');
const base = process.argv[2];
let failed = 0;
const ok = (cond, what, extra = '') => { console.log(`  ${cond ? 'ok  ' : 'FAIL'} ${what} ${extra}`); if (!cond) failed++; };

(async () => {
  const browser = await chromium.launch(require('./browser')());
  const ctx = await browser.newContext();               // one browser profile: tabs share channels
  const errors = [];
  // A freshly loaded sandboxed frame runs in its own process, and a mouse click sent in its first
  // moments can be lost before it reaches the frame. This test is about channels, not the mouse, so
  // it presses the app's buttons from inside the frame.
  const press = (frame, sel) => frame.locator(sel).dispatchEvent('click');
  const open = async (title) => {
    const p = await ctx.newPage();
    p.on('pageerror', e => errors.push(e.message));
    await p.goto(base);
    await p.click(`#apps button:has-text("${title}")`);
    return p;
  };

  // Tab A: the viewer suite asks to receive.
  const a = await open('Channel viewer');
  const kernel = a.frameLocator('.suiteframe');
  await kernel.locator('.wardian-perm').waitFor();
  ok(/read messages on the channel budget/.test(await kernel.locator('.wardian-perm').innerText()), 'viewer asks to read "budget"');
  await kernel.locator('.wardian-perm button.yes').click();

  // Tab B: the sender page asks to send.
  const b = await open('Channel sender');
  const page = b.frameLocator('.appframe');
  await press(page, '#send');
  await b.locator('.wardian-perm').waitFor();
  ok(/send messages on the channel budget/.test(await b.locator('.wardian-perm').innerText()), 'sender asks to send on "budget"');
  await b.locator('.wardian-perm button.yes').click();
  await page.locator('#out:has-text("sent")').waitFor();
  ok(true, 'sender sent');

  const view = kernel.frameLocator('iframe[title="view"]');
  await view.locator('#v:has-text("1798.65")').waitFor({ timeout: 5000 }).catch(() => {});
  ok((await view.locator('#v').textContent()) === '1798.65', 'viewer in the other tab received the message');
  ok((await view.locator('#from').textContent()) === 'from chan-sender', 'the sender is stamped by Wardian', `(${await view.locator('#from').textContent()})`);

  // ADR-2610091338 / SPEC 6.9.6: the host stamps an id; both sides see the same id and name.
  const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
  const receipt = (await page.locator('#receipt').textContent()).split(' ');
  const sentId = receipt[0];
  ok(UUID.test(sentId) && receipt.slice(1).join(' ') === 'Monthly budget', 'send() resolves to a UUID and the name', `(${receipt.join(' ')})`);
  ok(await view.locator('#id').textContent() === sentId && await view.locator('#name').textContent() === 'Monthly budget',
    'the receiver gets the same id and name', `(${await view.locator('#id').textContent()} ${await view.locator('#name').textContent()})`);

  await press(page, '#sneak');
  await page.locator('#out:has-text("error")').waitFor();
  ok(/not in app.json/.test(await page.locator('#out').textContent()), 'an undeclared channel is refused without asking');

  const grants = (await (await fetch(`${base}/api/grants`)).json()).grants;
  ok(grants.length === 2 && grants.every(g => g.allow), 'both answers are saved on the server', JSON.stringify(grants.map(g => `${g.app}:${g.mode}`)));

  // A late starter gets the latest message.
  const c = await open('Channel viewer');
  const late = c.frameLocator('.suiteframe').frameLocator('iframe[title="view"]');
  await late.locator('#v:has-text("1798.65")').waitFor({ timeout: 5000 }).catch(() => {});
  ok((await late.locator('#v').textContent()) === '1798.65', 'a viewer opened later gets the latest message without asking again');
  ok(await late.locator('#id').textContent() === sentId && await late.locator('#name').textContent() === 'Monthly budget',
    'a viewer opened later sees the same id and name', `(${await late.locator('#id').textContent()})`);

  const frameIn = (p, part) => p.frames().find(f => f.url().includes(part));
  const senderF = frameIn(b, '/apps/chan-sender/');
  const viewF = frameIn(a, '/frame/chan-viewer/view');
  const shown = async (p, sel) => p.locator(sel).textContent();
  const viewA = kernel.frameLocator('iframe[title="view"]');

  // SPEC 6.9.4: the host stamps each message with the sending package; a `from` the app sets is
  // replaced. The sender skips its library and posts the raw request, claiming to be chan-viewer.
  await senderF.evaluate(() => parent.postMessage({ wardian: 'ch', k: 'send', id: 'forged', channel: 'budget', data: { monthly: 42 }, from: 'chan-viewer' }, '*'));
  await viewA.locator('#v:has-text("42")').waitFor({ timeout: 5000 }).catch(() => {});
  ok(await viewA.locator('#v').textContent() === '42' && await viewA.locator('#from').textContent() === 'from chan-sender',
    'a `from` the app sets is replaced by the real package name', `(${await viewA.locator('#v').textContent()} ${await viewA.locator('#from').textContent()})`);
  // ADR-2610091338: the id is the host's, never taken from the request, and no name means null.
  const rawId = await viewA.locator('#id').textContent();
  ok(UUID.test(rawId) && rawId !== sentId && await viewA.locator('#name').textContent() === 'null',
    'each message gets a new id from the host, and a message with no name has name null', `(${rawId} ${await viewA.locator('#name').textContent()})`);
  const badNames = await senderF.evaluate(() => Promise.all([42, '   ', 'x'.repeat(121)].map(name =>
    wardian.channel('budget').send({ monthly: 0 }, { name }).then(() => 'sent', e => e.message))));
  ok(badNames.every(m => /name/.test(m)), 'a name that is not 1–120 characters is refused', `(${badNames.join(' | ')})`);

  // SPEC 6.9.4: every package allowed to receive gets it, except the sender.
  const selfSend = viewF.evaluate(() => window.sendBudget({ monthly: 'self' }));
  await kernel.locator('.wardian-perm').waitFor();
  await kernel.locator('.wardian-perm button.yes').click();
  const selfSent = await selfSend;
  await a.waitForTimeout(1000);
  const kept = (await (await fetch(`${base}/api/state/channel/budget`)).json()).message || {};
  ok(selfSent === 'sent' && kept.from === 'chan-viewer' && kept.data?.monthly === 'self' && UUID.test(kept.id) && 'name' in kept,
    'the viewer sent on budget, and the kept message has its id and name', `(${selfSent}, kept ${JSON.stringify(kept)})`);
  ok(await viewA.locator('#v').textContent() === '42' && await late.locator('#v').textContent() === '42',
    'the sending package does not receive its own message, in this tab or another', `(${await viewA.locator('#v').textContent()}, ${await late.locator('#v').textContent()})`);

  // SPEC 6.9: data is at most 256 KB.
  const big = await senderF.evaluate(() => wardian.channel('budget').send('x'.repeat(256 * 1024)).then(() => 'sent', e => e.message));
  ok(/at most 256 KB/.test(big), 'a send over 256 KB is refused', `(${big})`);
  // ADR-2610091338: the limit counts the whole message, so data just under it cannot pass the
  // browser and then be refused by the server.
  const edge = await senderF.evaluate(() => wardian.channel('budget').send('x'.repeat(256 * 1024 - 8)).then(() => 'sent', e => e.message));
  ok(/at most 256 KB/.test(edge), 'the limit counts the name and the host\'s stamps too', `(${edge})`);

  // SPEC 6.9: a package may send at most 100 messages in 10 seconds. A new tab starts its own count.
  const d = await open('Channel sender');
  let dF = null;
  for (let i = 0; i < 100 && !(dF = frameIn(d, '/apps/chan-sender/')); i++) await d.waitForTimeout(100);
  await dF.waitForFunction(() => typeof wardian === 'object');
  const burst = await dF.evaluate(async () => {
    const got = [];
    for (let i = 0; i < 101; i++) got.push(await wardian.channel('budget').send({ monthly: i }).then(() => 'sent', e => e.message));
    return got;
  });
  ok(burst.slice(0, 100).every(x => x === 'sent') && /too many/.test(burst[100]), 'the 101st message in 10 s is refused', `(${burst.filter(x => x === 'sent').length} sent, then: ${burst[100]})`);
  await d.close();

  // Revoke in Settings: the next send asks again; "Don't allow" is remembered and refuses.
  await b.click('#settingsBtn');
  await b.click('#setTab-permissions');
  await b.locator('#permList li:has-text("chan-sender") button').click();
  await b.waitForFunction(() => document.querySelectorAll('#permList li').length === 2);
  await b.click('#closeSettings');
  await press(page, '#send');
  await b.locator('.wardian-perm').waitFor();
  ok(true, 'after Revoke, the sender is asked again');
  // SPEC 6.9.2 / security.md Permissions: "Not now" refuses this use only and saves nothing.
  const before = JSON.stringify((await (await fetch(`${base}/api/grants`)).json()).grants);
  await b.locator('.wardian-perm button:has-text("Not now")').click();
  await page.locator('#out:has-text("not allowed")').waitFor({ timeout: 5000 }).catch(() => {});
  ok(/not allowed to send/.test(await page.locator('#out').textContent()), '"Not now" refuses this send');
  ok(JSON.stringify((await (await fetch(`${base}/api/grants`)).json()).grants) === before, '"Not now" saves nothing on the server');
  await press(page, '#send');
  await b.locator('.wardian-perm').waitFor({ timeout: 5000 }).catch(() => {});
  ok(await b.locator('.wardian-perm').count() === 1, 'after "Not now", the next send asks again');
  await b.locator('.wardian-perm button:has-text("Don\'t allow")').click();
  await page.locator('#out:has-text("not allowed")').waitFor({ timeout: 5000 }).catch(() => {});
  ok(/not allowed to send/.test(await page.locator('#out').textContent()), '"Don\'t allow" refuses the send');
  await press(page, '#send');
  await b.waitForTimeout(500);
  ok(await b.locator('.wardian-perm').count() === 0, '"Don\'t allow" is remembered: no second question');

  ok(errors.length === 0, 'no page errors', errors.join(' | '));
  await browser.close();
  console.log(failed ? `\n${failed} failed` : '\nall passed');
  process.exit(failed ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });

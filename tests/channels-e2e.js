// Channels between separate apps: a page app sends, a suite in another tab receives,
// and the user's permission is asked, remembered, revoked and refused.
// Usage: node tests/channels-e2e.js http://127.0.0.1:PORT   (apps folder must hold chan-sender and chan-viewer)
const { chromium } = require('playwright');
const base = process.argv[2];
let failed = 0;
const ok = (cond, what, extra = '') => { console.log(`  ${cond ? 'ok  ' : 'FAIL'} ${what} ${extra}`); if (!cond) failed++; };

(async () => {
  const browser = await chromium.launch({ channel: 'chrome' });
  const ctx = await browser.newContext();               // one browser profile: tabs share channels
  const errors = [];
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
  await page.locator('#send').click();
  await b.locator('.wardian-perm').waitFor();
  ok(/send messages on the channel budget/.test(await b.locator('.wardian-perm').innerText()), 'sender asks to send on "budget"');
  await b.locator('.wardian-perm button.yes').click();
  await page.locator('#out:has-text("sent")').waitFor();
  ok(true, 'sender sent');

  const view = kernel.frameLocator('iframe[title="view"]');
  await view.locator('#v:has-text("1798.65")').waitFor({ timeout: 5000 }).catch(() => {});
  ok((await view.locator('#v').textContent()) === '1798.65', 'viewer in the other tab received the message');
  ok((await view.locator('#from').textContent()) === 'from chan-sender', 'the sender is stamped by Wardian', `(${await view.locator('#from').textContent()})`);

  await page.locator('#sneak').click();
  await page.locator('#out:has-text("error")').waitFor();
  ok(/not in app.json/.test(await page.locator('#out').textContent()), 'an undeclared channel is refused without asking');

  const grants = (await (await fetch(`${base}/api/grants`)).json()).grants;
  ok(grants.length === 2 && grants.every(g => g.allow), 'both answers are saved on the server', JSON.stringify(grants.map(g => `${g.app}:${g.mode}`)));

  // A late starter gets the latest message.
  const c = await open('Channel viewer');
  const late = c.frameLocator('.suiteframe').frameLocator('iframe[title="view"]');
  await late.locator('#v:has-text("1798.65")').waitFor({ timeout: 5000 }).catch(() => {});
  ok((await late.locator('#v').textContent()) === '1798.65', 'a viewer opened later gets the latest message without asking again');

  // Revoke in Settings: the next send asks again; "Don't allow" is remembered and refuses.
  await b.click('#settingsBtn');
  await b.locator('#permList li:has-text("chan-sender") button').click();
  await b.waitForFunction(() => document.querySelectorAll('#permList li').length === 1);
  await b.click('#closeSettings');
  await page.locator('#send').click();
  await b.locator('.wardian-perm').waitFor();
  ok(true, 'after Revoke, the sender is asked again');
  await b.locator('.wardian-perm button:has-text("Don\'t allow")').click();
  await page.locator('#out:has-text("not allowed")').waitFor({ timeout: 5000 }).catch(() => {});
  ok(/not allowed to send/.test(await page.locator('#out').textContent()), '"Don\'t allow" refuses the send');
  await page.locator('#send').click();
  await b.waitForTimeout(500);
  ok(await b.locator('.wardian-perm').count() === 0, '"Don\'t allow" is remembered: no second question');

  ok(errors.length === 0, 'no page errors', errors.join(' | '));
  await browser.close();
  console.log(failed ? `\n${failed} failed` : '\nall passed');
  process.exit(failed ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });

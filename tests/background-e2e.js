// End-to-end test of "Make an app" in the background: you leave the chat while Claude works, the
// browser test runs out of sight, a failure goes back to Claude, and the button says how it went.
const { chromium } = require('playwright');
const B = process.env.BASE, FAKE = process.env.FAKE, SRC = process.env.SRC, WORKING = process.env.WORKING;
const nodefs = require('fs');
let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('  ok  ', m); } else { fail++; console.log('  FAIL', m); } };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const post = (path, body) => fetch(B + path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }).then(r => r.json());
const sessions = () => fetch(B + '/api/ai/sessions').then(r => r.json());
async function until(fn, ms, what) {
  for (let t = 0; t < ms / 250; t++) { if (await fn()) return true; await sleep(250); }
  console.log('  (gave up waiting for ' + what + ')'); return false;
}

(async () => {
  // PROVIDER=bedrock runs the same build through the fake Bedrock (ADR-2610071106), signed in with
  // access keys or, with BEDROCK_AUTH=profile-…, an AWS profile (ADR-2610091530).
  const AUTH = process.env.BEDROCK_AUTH || 'access-keys';
  const PROFILE = process.env.PROVIDER === 'bedrock' && AUTH.startsWith('profile-');
  if (PROFILE) {
    const profiles = await (await fetch(B + '/api/ai/aws-profiles')).json();
    ok(profiles.profiles.some(p => p.name === 'e2e' && p.region === 'us-east-1') && !/ASIA|secret|session-token/.test(JSON.stringify(profiles)), 'the profiles are listed with their regions, no keys: ' + JSON.stringify(profiles.profiles));
    ok(AUTH === 'profile-cli' ? /\/aws$/.test(profiles.cli || '') : profiles.cli === null, 'the AWS CLI is found on PATH, or not used: ' + profiles.cli);
    let r = await post('/api/ai/provider', { provider: 'bedrock', region: '', auth: 'profile', profile: 'nobody' });
    ok(/^there is no AWS profile "nobody" in .*\/home\/\.aws\/config or .*\/home\/\.aws\/credentials \(not there\)$/.test(r.error || ''), 'an unknown profile names the files read: ' + r.error);
    if (AUTH === 'profile-cli') {
      r = await post('/api/ai/provider', { provider: 'bedrock', region: '', auth: 'profile', profile: 'e2e-expired' });
      ok(r.error === 'the AWS sign-in of profile "e2e-expired" has expired. Run `aws sso login --profile e2e-expired`, then try again', 'an expired SSO sign-in says what to run: ' + r.error);
    }
  }
  const k = process.env.PROVIDER !== 'bedrock' ? await post('/api/ai/key', { key: 'test-key' })
    : PROFILE ? await post('/api/ai/provider', { provider: 'bedrock', region: '', auth: 'profile', profile: 'e2e' })
    : await post('/api/ai/provider', { provider: 'bedrock', region: 'us-east-1', auth: 'access-keys', access_key_id: 'AKIDTEST', secret_access_key: 'test-secret' });
  if (PROFILE) ok(k.bedrock && k.bedrock.settings.auth === 'profile' && k.bedrock.settings.profile === 'e2e' && k.bedrock.settings.region === 'us-east-1', 'the profile is tested and saved, in its own region: ' + JSON.stringify(k.bedrock && k.bedrock.settings));
  ok(k.ready && k.provider === (process.env.PROVIDER || 'anthropic'), 'Claude is set up: ' + (process.env.PROVIDER || 'anthropic'));
  const browser = await chromium.launch(require('./browser')({ headless: true }));
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const a = await context.newPage();
  const errors = [];
  a.on('pageerror', e => errors.push(e.message));
  await a.goto(B + '/'); await sleep(800);

  console.log('== start a chat, then leave it');
  await a.click('#aiBtn');
  await a.fill('#aiInput', 'a small app that says hello');
  await a.click('.ai-wrap button.primary:has-text("Send")');
  await sleep(300);
  await a.click('#apps li.app .app-open');                       // open another app while Claude works
  const other = await a.locator('#runner h2').first().textContent();
  ok(!(await a.locator('#chat').count()), 'you are on another app: ' + other);
  await until(async () => /working/.test(await a.locator('#aiBtn').textContent()), 5000, 'working');
  ok(/working/.test(await a.locator('#aiBtn').textContent()), 'the button says Claude is working: ' + await a.locator('#aiBtn').textContent());

  console.log('== a second tab sees the same chat');
  const b = await context.newPage();
  b.on('pageerror', e => errors.push(e.message));
  await b.goto(B + '/'); await sleep(1500);
  ok(/working|testing/.test(await b.locator('#aiBtn').textContent()), 'the second tab shows it too: ' + await b.locator('#aiBtn').textContent());

  console.log('== the test runs out of sight; its failure goes back to Claude');
  const done = await until(async () => { const s = (await sessions())[0]; return s && s.state === 'ready'; }, 60000, 'ready');
  ok(done, 'the chat ends ready, with nobody on the chat page');
  const s = (await sessions())[0];
  const ev = (await (await fetch(B + `/api/ai/events?session=${s.session}&since=0`)).json()).events;
  const kinds = ev.filter(e => e.kind !== 'tool' && e.kind !== 'say').map(e => e.kind + (e.kind === 'tested' ? (e.ok ? '+' : '-') : ''));
  ok(kinds.join() === 'user,saved,testing,tested-,test,saved,testing,tested+', 'one test per save, the failure sent back, then a pass: ' + kinds.join());
  ok(/boom: the first version is broken/.test(ev.find(e => e.kind === 'test').text), 'Claude got the real error');
  ok((await (await fetch(FAKE + '/log')).json()).join() === 'write,components,finish,done,fix,finish,done', 'Claude was asked twice: build, then fix');
  const suiteJson = await (await fetch(B + '/apps/bg-test/suite.json')).json();
  ok(suiteJson.styles && suiteJson.styles[0] === 'ui/theme.css' && (await fetch(B + '/apps/bg-test/ui/button.css')).ok, 'the app it built uses the component library: ' + JSON.stringify(suiteJson.styles));
  ok((await a.locator('#runner h2').first().textContent()) === other, 'your other app stayed open the whole time');
  await until(async () => /ready/.test(await a.locator('#aiBtn').textContent()), 5000, 'ready badge');
  ok(/ready/.test(await a.locator('#aiBtn').textContent()) && await a.locator('#aiBtn').getAttribute('data-state') === 'ready', 'the button says ready');

  console.log('== back to the chat');
  await a.click('#aiBtn');
  await a.locator('#chat .msg.test.ok').waitFor({ timeout: 5000 });
  const chatText = await a.locator('#chat').textContent();
  ok(/found 1 problem/.test(chatText) && /Sent to Claude/.test(chatText) && /Browser test passed/.test(chatText), 'the chat shows what happened while you were away');
  await a.locator('#aiPreview iframe').waitFor({ timeout: 5000 });
  ok(await a.locator('#aiPreview iframe').count() === 1, 'and the finished app below it');
  await sleep(500);
  ok(await a.locator('#aiBtn').getAttribute('data-state') === '', 'the badge clears once you have seen it');
  await b.reload(); await sleep(1500);
  ok(await b.locator('#aiBtn').getAttribute('data-state') === '', 'in the other tab too');
  console.log('== the working folder and the app\'s history (ADR-2610071122)');
  ok((await (await fetch(B + '/api/apps')).json()).includes('adder') && nodefs.existsSync(WORKING + '/adder/app.wasm'), 'the working folder was filled from ./apps');
  ok(nodefs.existsSync(WORKING + '/bg-test/suite.json') && !nodefs.existsSync(SRC + '/bg-test'), 'the new app is saved in the working folder, not in ./apps');
  ok(nodefs.readdirSync(SRC).join() === 'adder', './apps is untouched: ' + nodefs.readdirSync(SRC).join());
  ok(/Version 2; the earlier versions are in/.test(await a.locator('#chat').textContent()), 'the chat says which version the fix is');
  let hist = await (await fetch(B + '/api/history/bg-test')).json();
  ok(hist.versions.map(v => v.n + ':' + v.by).join() === '2:make-an-app,1:make-an-app' && hist.versions[0].why === 'A small test app.' && hist.versions[0].current, 'the build and its fix are two versions, with Claude\'s reason: ' + JSON.stringify(hist.versions.map(v => [v.n, v.by, v.why])));
  await a.locator('#apps li button', { hasText: /Background test/ }).first().click();
  await a.locator('#historyBtn').click();
  await a.locator('#historyList .ver').first().waitFor({ timeout: 5000 });
  ok(await a.locator('#historyList .ver').count() === 2 && /current/.test(await a.locator('#historyList .ver').first().textContent()), 'the History panel lists both, newest first');
  const v1 = a.locator('#historyList .ver[data-n="1"]');
  await v1.locator('button', { hasText: 'Compare with current' }).click();
  await v1.locator('pre.diff').first().waitFor({ timeout: 5000 });
  const diffText = await v1.locator('.diffbox').textContent();
  ok(/apps\/main\/app\.js/.test(diffText) && /boom: the first version is broken/.test(diffText) && /fixed/.test(diffText), 'Compare shows app.js changed, old and new lines');
  await v1.locator('button', { hasText: 'Restore' }).click();
  await v1.locator('button', { hasText: 'Restore version 1' }).click();
  await a.locator('iframe.suiteframe').waitFor({ timeout: 5000 });
  hist = await (await fetch(B + '/api/history/bg-test')).json();
  ok(hist.versions[0].n === 3 && hist.versions[0].by === 'restore' && hist.versions[0].why === 'restored version 1', 'restoring is saved as version 3');
  ok(/boom: the first version is broken/.test(await (await fetch(B + '/apps/bg-test/apps/main/app.js')).text()), 'and the first app.js is back');
  ok(!nodefs.existsSync(SRC + '/bg-test') && nodefs.readdirSync(SRC).join() === 'adder', './apps is still untouched');

  if (PROFILE) {
    console.log('== Settings → Claude shows the AWS profile');
    await a.click('#settingsBtn');
    await a.locator('#settings[open]').waitFor({ timeout: 3000 });
    await a.click('#setTab-claude');
    ok(/AWS profile e2e, from these settings/.test(await a.locator('#aiKeyState').textContent()), 'the status says AWS profile e2e: ' + await a.locator('#aiKeyState').textContent());
    await a.locator('#bdProfile option[value="e2e"]').waitFor({ state: 'attached', timeout: 5000 }).catch(() => {});
    ok(await a.locator('#bdAuth').inputValue() === 'profile' && await a.locator('#bdProfile').inputValue() === 'e2e' && await a.locator('#bdProfileRow').isVisible(), 'the form shows the profile chosen');
    ok((await a.locator('#bdProfile option').allTextContents()).at(-1) === 'Other…', 'with Other… for a profile not in the list');
    await a.fill('#bdRegion', '');
    await a.selectOption('#bdProfile', '');
    ok(await a.locator('#bdProfileOther').isVisible(), 'Other… asks for a name');
    await a.selectOption('#bdProfile', 'e2e');
    ok(await a.locator('#bdRegion').inputValue() === 'us-east-1', 'the region is filled in from the profile');
    // Choosing another profile moves the region with it, though the box already holds one.
    if (await a.locator('#bdProfile option[value="default"]').count()) {
      await a.selectOption('#bdProfile', 'default');
      ok(await a.locator('#bdRegion').inputValue() === 'eu-west-3', 'choosing another profile sets its region: ' + await a.locator('#bdRegion').inputValue());
      await a.selectOption('#bdProfile', 'e2e');
      ok(await a.locator('#bdRegion').inputValue() === 'us-east-1', 'and back again: ' + await a.locator('#bdRegion').inputValue());
    }
    await a.click('#bdSave');
    await a.locator('#aiKeyMsg', { hasText: /Bedrock works|Not saved/ }).waitFor({ timeout: 10000 }).catch(() => {});
    ok(/Bedrock works/.test(await a.locator('#aiKeyMsg').textContent()), 'Test and use Bedrock works with the profile: ' + await a.locator('#aiKeyMsg').textContent());
    await a.click('#closeSettings');
  }
  if (process.env.PROVIDER === 'bedrock') {
    const seen = await (await fetch(process.env.BEDROCK + '/seen')).json();
    ok(seen.length >= 6 && seen.every(x => x === 'sigv4'), 'every Bedrock request was signed with SigV4 and the signature checked: ' + seen.length + ' requests');
    const signed = await (await fetch(process.env.BEDROCK + '/signed')).json();
    const want = PROFILE ? 'ASIAPROFILETEST/profile-test-session-token' : 'AKIDTEST/';
    ok(signed.length === seen.length && signed.every(x => x.key_id + '/' + x.session_token === want), 'each was signed with ' + want + ': ' + [...new Set(signed.map(x => x.key_id + '/' + x.session_token))]);
    const st = await (await fetch(B + '/api/status')).json();
    ok(!/test-secret|profile-test-session-token|ASIAPROFILETEST/.test(JSON.stringify(st)), '/api/status shows no key, secret or session token');
  }
  if (PROFILE) {
    const ran = nodefs.readFileSync(process.env.AWS_LOG, 'utf8').trim().split('\n');
    const what = AUTH === 'profile-cli' ? 'aws configure export-credentials --profile e2e --format process' : "creds --profile e2e run";
    const mine = ran.filter(l => l === what);
    ok(mine.length >= 1 && mine.length <= 3, `the keys came from ${AUTH === 'profile-cli' ? 'the AWS CLI' : 'the credential_process'}, kept in memory between requests: ${mine.length} runs for ${(await (await fetch(process.env.BEDROCK + '/seen')).json()).length} requests (${ran.join('; ')})`);
  }
  ok(errors.length === 0, 'no page errors ' + JSON.stringify(errors));
  await browser.close();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });

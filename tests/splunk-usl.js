// The Splunk table app's "Ready for the USL lab?" check (ADR-2610121400), without a browser: it finds
// the load, throughput and response time columns of the tables its templates make, and warns by
// Gunther's rules: a load that is a result (a utilisation), a count for a rate, raw rows for one row
// per load, too few loads, superlinear scaling, no peak, Little's Law, and rows on little data.
//
//   node tests/splunk-usl.js
'use strict';
const fs = require('fs'), vm = require('vm'), path = require('path'), assert = require('assert/strict');

const dir = path.join(__dirname, '..', 'apps', 'splunk-table', 'shared');
const ctx = {}; vm.createContext(ctx);
for (const f of ['cells.js', 'sample.js', 'usl.js']) vm.runInContext(fs.readFileSync(path.join(dir, f), 'utf8'), ctx);
vm.runInContext('this.UslReady = UslReady; this.SampleTable = SampleTable;', ctx);
const { UslReady, SampleTable } = ctx;
let failed = 0;
function test(name, fn){
  try { fn(); console.log('ok   ' + name); } catch (e) { failed++; console.log('FAIL ' + name + '\n     ' + e.message); }
}
const usl = (n, a, b) => n / (1 + a*(n - 1) + b*n*(n - 1));
// A load test with a peak: throughput rises, then falls (α 0.03, β 0.001, peak near 31).
const curve = (loads, f = n => 1000*usl(n, 0.03, 0.001)) => loads.map(n => [String(n), f(n).toFixed(2)]);
const LOADS = [1, 2, 4, 8, 16, 24, 32, 48, 64];
const levels = r => r.findings.map(f => f.level + ': ' + f.text).join('\n     ');
const says = (r, level, re) => assert.ok(r.findings.some(f => f.level === level && re.test(f.text)), `expected ${level} ${re}, got\n     ${levels(r)}`);
const silent = (r, re) => assert.ok(!r.findings.some(f => re.test(f.text)), `did not expect ${re}, got\n     ${levels(r)}`);

test('the sample table and each template shape are ready, with their columns', () => {
  const s = SampleTable.make();
  const r = UslReady.assess(s.fields, s.rows);
  assert.deepEqual({...r.use}, {load: 'concurrency', throughput: 'throughput', response: 'response_ms'});
  assert.ok(r.ready, levels(r));
  says(r, 'ok', /Little's Law.*milliseconds/);
  const t = UslReady.assess(['n', 'x', 'r', 'minutes'], LOADS.map(n => [n, 1000*usl(n, 0.03, 0.001), n / (1000*usl(n, 0.03, 0.001)) * 1000, 30]));
  assert.deepEqual({...t.use}, {load: 'n', throughput: 'x', response: 'r'});
  assert.ok(t.ready, levels(t));
  says(t, 'ok', /9 load levels, from 1 to 64/);
  says(t, 'ok', /peaks at a load of 32/);
});

test('JMeter and k6 names are found: allThreads, vus, tps, avg_elapsed', () => {
  assert.deepEqual({...UslReady.assess(['allThreads', 'tps', 'avg_elapsed'], curve(LOADS).map(r => [...r, '50'])).use}, {load: 'allThreads', throughput: 'tps', response: 'avg_elapsed'});
  assert.deepEqual({...UslReady.assess(['vus', 'http_reqs_per_sec'], curve(LOADS)).use}, {load: 'vus', throughput: 'http_reqs_per_sec', response: ''});
});

test('a utilisation or load average is never the load, and says why', () => {
  const r = UslReady.assess(['cpu_utilization', 'throughput'], curve(LOADS));
  assert.equal(r.ready, false);
  says(r, 'problem', /cpu_utilization is a result of the load, not the load/);
  const given = UslReady.assess(['load_avg', 'throughput'], curve(LOADS), {load: 'load_avg', throughput: 'throughput'});
  says(given, 'problem', /load_avg, looks like a utilisation or a load average/);
});

test('a count is not a rate; unnamed columns are guessed and said so', () => {
  says(UslReady.assess(['users', 'count'], curve(LOADS)), 'watch', /count looks like a count, not a rate/);
  const g = UslReady.assess(['a', 'b'], curve(LOADS));
  assert.deepEqual({...g.use}, {load: 'a', throughput: 'b', response: ''});
  says(g, 'watch', /first two number columns are used: a and b/);
});

test('raw rows over time, and too few loads', () => {
  const raw = UslReady.assess(['_time', 'threads', 'throughput'], [1, 1, 1, 2, 2, 2, 4, 4, 4, 8, 8, 8].map((n, i) => [String(i), String(n), String(100*n)]));
  says(raw, 'watch', /12 rows for 4 loads.*stats avg\(throughput\) AS throughput BY threads/);
  const few = UslReady.assess(['threads', 'throughput'], curve([1, 2]));
  assert.equal(few.ready, false);
  says(few, 'problem', /Only 2 load levels/);
  says(UslReady.assess(['threads', 'throughput'], curve([1, 4, 8, 16])), 'watch', /4 load levels\. Gunther advises at least half a dozen/);
  says(UslReady.assess(['threads', 'throughput'], curve([10, 12, 14, 16, 18, 20])), 'watch', /span only ×2/);
});

test('superlinear scaling, no peak, and a load of 1 not measured', () => {
  says(UslReady.assess(['threads', 'throughput'], curve(LOADS, n => 100*n*(n < 8 ? 1 : 1.5))), 'watch', /superlinear/);
  const rising = UslReady.assess(['threads', 'throughput'], curve([1, 2, 4, 8, 12, 16], n => 1000*usl(n, 0.05, 0)));
  says(rising, 'watch', /still rising at the highest load/);
  silent(rising, /superlinear/);
  says(UslReady.assess(['threads', 'throughput'], curve([4, 8, 16, 32, 48, 64])), 'ok', /No run at a load of 1/);
});

test("Little's Law: think time is allowed for, but named", () => {
  says(UslReady.assess(['users', 'throughput', 'latency_ms'], curve(LOADS).map(r => [...r, '5'])), 'watch', /Little's Law.*think time/);
  says(UslReady.assess(['n', 'x', 'r'], LOADS.map(n => { const x = 1000*usl(n, 0.03, 0.001); return [n, x, n / x]; })), 'ok', /Little's Law, with the response time in seconds/);
});

test('rows resting on little data are named; blank and zero rows are counted', () => {
  const r = UslReady.assess(['n', 'x', 'minutes'], LOADS.map(n => [n, 1000*usl(n, 0.03, 0.001), n === 64 ? 1 : 40]));
  says(r, 'watch', /1 load rests on little data \(minutes under 4\)/);
  const z = UslReady.assess(['n', 'x'], [...curve(LOADS), ['0', '10'], ['', '']]);
  says(z, 'watch', /2 rows have no load or throughput above 0/);
});

test('a table with no numbers says what is missing', () => {
  const r = UslReady.assess(['host', 'status'], [['a', 'ok'], ['b', 'down']]);
  assert.equal(r.use, null);
  assert.equal(r.ready, false);
  says(r, 'problem', /No load column/);
  says(r, 'problem', /No throughput column/);
});

if (failed){ console.log(failed + ' failed'); process.exit(1); }
console.log('all passed');

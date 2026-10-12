// The USL lab's maths, without a browser (ADR-2610121300): the fit recovers known parameters from
// exact data, the formatter keeps the significant figures it is asked for at every size, the band
// is interpolated like the ranges, and a full analysis with its refits stays quick.
//
//   node tests/usl-math.js
'use strict';
const fs = require('fs'), vm = require('vm'), path = require('path'), assert = require('assert/strict');

const dir = path.join(__dirname, '..', 'apps', 'usl-lab', 'core');
const ctx = {}; vm.createContext(ctx);
vm.runInContext(fs.readFileSync(path.join(dir, 'lib.js'), 'utf8') + ';this.Lib = Lib;', ctx);
vm.runInContext(fs.readFileSync(path.join(dir, 'engine.js'), 'utf8') + ';this.UslEngine = UslEngine;', ctx);
const { Lib, UslEngine } = ctx;
const rel = (a, b) => Math.abs(a - b) / Math.abs(b);
let failed = 0;
function test(name, fn){
  try { fn(); console.log('ok   ' + name); } catch (e) { failed++; console.log('FAIL ' + name + '\n     ' + e.message); }
}

const BOOTS = 1000, SEED = 20240611;   // what the session app asks for
const analyze = (rows, boots = BOOTS) => UslEngine.handle('analyze', {id: 1, rows, rdiv: 1000, boots, seed: SEED});

test('the fit recovers known parameters from exact data to 1e-6', () => {
  const cases = [
    ['the example system', 980, 0.04, 0.0006, [1, 2, 4, 8, 12, 16, 24, 32, 40, 48, 56, 64]],
    ['low contention', 1200, 0.002, 0.00002, [1, 2, 4, 8, 16, 32, 64, 128, 256]],
    ['a very small coherency', 500, 0.08, 1e-7, [1, 4, 8, 16, 32, 64, 128]],
    ['strong coherency', 300, 0.1, 0.01, [1, 2, 3, 4, 6, 8, 10, 12, 16]],
    ['large loads', 50, 0.0005, 2e-7, [1, 10, 50, 100, 250, 500, 1000, 2000, 4000]],
  ];
  for (const [name, lambda, alpha, beta, ns] of cases){
    const f = analyze(ns.map(n => ({n, x: lambda*Lib.model(n, alpha, beta), r: null})), 0).fit;
    const want = Lib.peakInfo(lambda, alpha, beta);
    for (const [k, got, exp] of [['α', f.alpha, alpha], ['β', f.beta, beta], ['λ', f.lambda, lambda], ['N*', f.peak.nStar, want.nStar], ['X max', f.peak.xMax, want.xMax]])
      assert.ok(rel(got, exp) < 1e-6, `${name}: ${k} is ${got}, should be ${exp} (relative error ${rel(got, exp).toExponential(1)})`);
  }
});

test('fmt keeps the significant figures asked for, small, middling and large', () => {
  const cases = [
    [0.0006, 4, '6e-4'], [0.000612345, 4, '6.123e-4'], [0.0006, 2, '6e-4'], [1.23456e-7, 3, '1.23e-7'],
    [0.04123456, 4, '0.04123'], [0.04, 4, '0.04'], [0.123456789, 6, '0.123457'], [980.123456, 5, '980.12'],
    [1234567, 5, '1,234,600'], [25300000000, 3, '2.53e+10'], [12.5, undefined, '12.5'], [-0.0612345, 3, '-0.0612'],
    [0, 4, '0'], [Infinity, 4, 'no limit'], [NaN, 4, '–'],
  ];
  for (const [v, sig, want] of cases) assert.equal(Lib.fmt(v, sig), want, `fmt(${v}, ${sig})`);
});

const example = (() => {
  const noise = [0.004, -0.012, 0.010, -0.008, 0.015, -0.006, -0.17, 0.012, -0.010, 0.009, -0.014, 0.011];
  return [1, 2, 4, 8, 12, 16, 24, 32, 40, 48, 56, 64].map((n, i) => ({n, x: Math.round(980*Lib.model(n, 0.04, 0.0006)*(1 + noise[i])), r: null}));
})();

test('a full analysis of the example data, with 1000 refits, takes under 3 seconds', () => {
  const t = Date.now();
  const A = analyze(example);
  const ms = Date.now() - t;
  assert.equal(A.samples, BOOTS);
  assert.ok(ms < 3000, `took ${ms} ms`);
  for (const k of ['alpha', 'beta', 'lambda', 'nStar', 'xMax']) assert.ok(A.ci[k][0] <= A.ci[k][1], `${k} range is in order`);
  console.log(`     (${ms} ms)`);
});

test('the band is interpolated between refits, contains the fit, and is the same each time', () => {
  const A = analyze(example);
  const f = A.fit, ns = example.map(r => r.n);
  const c = UslEngine.handle('curve', {id: 1, sets: [{lambda: f.lambda, alpha: f.alpha, beta: f.beta}], ns, band: [0.05, 0.95]});
  ns.forEach((n, i) => assert.ok(c.low[i] <= c.ys[0][i] && c.ys[0][i] <= c.high[i], `at ${n}: ${c.low[i]} ≤ ${c.ys[0][i]} ≤ ${c.high[i]}`));
  // Between the 5% and 6% points of 1000 refits the band moves smoothly, as an interpolated percentile does.
  const at = q => UslEngine.handle('curve', {id: 1, sets: [], ns: [ns[6]], band: [q, 0.95]}).low[0];
  const a = at(0.05), b = at(0.0505), d = at(0.051);
  assert.ok(Math.abs((b - a) - (d - b)) <= 1e-9*Math.abs(a) || (a < b && b < d), `5%: ${a}, 5.05%: ${b}, 5.1%: ${d}`);
  const again = UslEngine.handle('curve', {id: 1, sets: [], ns, band: [0.05, 0.95]});
  assert.deepEqual([again.low, again.high], [c.low, c.high]);
});

if (failed){ console.log(failed + ' failed'); process.exit(1); }
console.log('all passed');

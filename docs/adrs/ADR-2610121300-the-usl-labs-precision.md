# ADR-2610121300: the USL lab's precision

**Status:** Accepted
**Date:** 2026-10-12
**Drivers:** The user: "we need more mathematical precision", then "yes, do all of it" to the
changes below.

## Context

Measured on exact data with known parameters, the lab's fit recovers α, β, λ, N* and the peak to
about 1e-10 relative, and 2e-7 at worst, for a coherency of 1e-7. The quick refits behind the 90%
ranges agree with a full fit to about 1e-7. The numbers are 64-bit doubles, the same as WebAssembly
would use. So precision was not lost in the calculation, but in what the lab showed and wrote:

- `fmt` ignored the significant figures asked for below 0.001 and from 1e9: always 2 and 3. So
  β = 0.0006 was `6.0e-4` everywhere.
- The readouts gave α and β 2 significant figures, N* 3. The report gave α and β 3.
- The curve's CSV had 3 decimals, and its efficiency 4.
- The band took the nearest of the 240 refits, while the ranges interpolate. So the two could
  disagree, and 240 refits leave a 5th or 95th percentile noisy.

## Decision

1. **`fmt` honours its significant figures at every size:** exponent form below 0.001 and from 1e9,
   trailing zeros dropped (`6e-4`, `6.123e-4`), and grouping in between.
2. **One table of significant figures, `Lib.SIG`:** α 4, β 4, λ 5, N* 4, peak throughput 5. The
   readouts and what goes to Claude use it. The report uses 6 (`Lib.SIG_REPORT`) for every
   parameter, its range, and R².
3. **The CSV has full precision:** each number is the shortest text that reads back as the same double.
4. **The band is interpolated** with the same percentile function as the ranges.
5. **1000 refits instead of 240.** An analysis of the example data takes about a quarter of a second, in the background worker.
6. **A test of the maths without a browser,** `tests/usl-math.js`, run by CI's fast job and
   `tests/run-all.sh`:
   - known parameters come back to 1e-6 from exact data, over five systems;
   - `fmt` keeps its figures;
   - a full analysis stays under 3 seconds;
   - the band is interpolated, contains the fit, and is the same each time.

## Consequences

- People see the digits the fit actually has. A spreadsheet that reads the CSV gets the curve exactly.
- The ranges are steadier from one small change in the data to the next.
- An analysis takes about four times as long as before, still well under a second for typical data.
- The digits shown are more than the data justifies when the ranges are wide. The ranges beside
  them say so.

## Implementation

`apps/usl-lab/core/lib.js` (`fmt`, `SIG`, `SIG_REPORT`), `core/engine.js` (`curve`),
`apps/readouts`, `apps/export`, `apps/diagnosis`, `apps/session` (1000 refits), `tests/usl-math.js`,
`tests/run-all.sh`, `.github/workflows/ci.yml`.

## Gate

`node tests/usl-math.js`

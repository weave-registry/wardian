# ADR-2610121400: the Splunk table checks data for the USL

**Status:** Accepted
**Date:** 2026-10-12
**Drivers:** The user: "i need additional support in the splunk app to ensure it can find the
relevant data for https://www.perfdynamics.com/Manifesto/USLscalability.html or warns".

## Context

The Splunk table app sends its table to the USL lab, which fits the Universal Scalability Law. For a
table made from a template, or written by Claude, the lab is told which columns are the load, the
throughput and the response time. For any other table it took the first three columns, and nothing
said whether the data suited the USL at all.

Gunther's page sets what the data must be:
- **The load:** N is the independent variable: users, load generators or processors, never a
  dependent one such as a utilisation.
- **The throughput:** X(N) comes from measurements near steady state.
- **How many points:** at least half a dozen.
- **X(1):** it need not be measured, since the three-parameter form fits it.
- **When the data diverge from the model:** stop measuring and find out why.

## Decision

1. **`shared/usl.js` (`UslReady.assess`)** finds the columns by name, with JMeter, k6 and Splunk
   names in its lists (`allThreads`, `vus`, `tps`, `elapsed`, and so on). When no name fits, it uses
   the first two number columns and says so. Its findings are ok, watch or problem:
   - **Problems:**
     - no load or no throughput column;
     - a load that is a utilisation, a CPU percentage, a load average or a queue length, with why;
     - fewer than 3 load levels, or 4 runs.
   - **To watch:**
     - a count where a rate belongs;
     - several rows per load, or `_time`, with the `stats … BY` that fixes it;
     - fewer than 6 load levels, or loads spanning less than ×4;
     - rows without a load or throughput above 0;
     - throughput per unit of load that rises by more than 10% (superlinear), or never peaks;
     - load against throughput × response time off by more than 25% in both ms and s (Little's
       Law, with think time named as an innocent cause);
     - loads resting on under a tenth of the median data, from a minutes or count column.
   - **OK:** the number and range of loads, a peak in the data, no run at a load of 1 (the fit
     estimates it), and Little's Law holding.
2. **About shows it, as "Ready for the USL lab?":** a one-line verdict and the findings. It is
   open when the table came from a template or Claude, or a load and a throughput are found. A table
   in the database is checked from its first 1,000 rows: About gains `db`.
3. **Send to other apps adds `use`** (the columns found) when the search did not say. The lab
   already preselects from it, and the user can still change them there.
4. **A JMeter template:** per minute, the samples and their mean elapsed time, keeping only minutes in
   which `allThreads` did not change. So ramps between steps are left out and each load is near steady state.
5. **Ask Claude's USL rules** gain Gunther's: the load is independent, never a utilisation; minutes
   with a changing load are left out; throughput is a rate over equal windows; aim for 6 or more
   levels.

## Consequences

- An arbitrary Splunk table goes to the lab with sensible columns, or with a reason why it can't.
- The checks are heuristics on column names and values. A good table with odd names gets a "guessed"
  note, and the lab's own checks still run after the fit.

## Implementation

`apps/splunk-table/shared/usl.js`, `apps/about` (view, app, `db`), `apps/keep` (`use`),
`apps/search/presets.js` and `view.html` (JMeter), `apps/ask` (rules), `suite.json`, `style.css`,
`README.md`; `tests/splunk-usl.js`, `tests/splunk-e2e.js`, `tests/run-all.sh`, `.github/workflows/ci.yml`.

## Gate

`node tests/splunk-usl.js`; `tests/run-splunk-e2e.sh` (About's verdict for a search and for the
traffic template, and the JMeter template's steady-minutes search).

# Loan planner

A rustle **suite**: six sealed apps that plan a loan together. WebAssembly does the amortization.

| App | Slot | Job | Contract |
|---|---|---|---|
| `inputs` | aside | Amount, rate, term, extra payment, start month, currency. Checks and remembers them. | emits `loan:changed` (retained); `storage` |
| `engine` | — | Runs `engine.wasm`: the monthly payment and a row per month. | provides `schedule`; `asset` |
| `planner` | — | Turns each change into one plan for every viewer, with and without the extra payment. | listens `loan:changed`; needs `engine.schedule`; emits `plan:ready` (retained) |
| `summary` | main | Monthly payment, total interest, total paid, payoff date, what the extra saves. | listens `plan:ready` |
| `chart` | main | Balance over time, with and without the extra payment. Hover or use the arrow keys. | listens `plan:ready` |
| `export` | main | Downloads the schedule as CSV. | listens `plan:ready`; `claude:downloads` |

How a change flows: **inputs** emits `loan:changed` → **planner** calls `engine.schedule` twice
(as asked, and without the extra payment) → **planner** emits `plan:ready` → **summary**,
**chart** and **export** each draw their part. The engine runs once per change, not once per viewer.

`shared/format.js` (money, months, durations) is listed in `suite.json` "scripts", so every frame
gets its own copy. Apps never share objects.

The engine's source is `src/lib.rs`: `payment`, `schedule`, `alloc` and `dealloc`. Rebuild
`engine.wasm` with `./build.sh`. Its buffers are 8-byte aligned so JavaScript can read them as a
`Float64Array`.

The numbers use the standard formula with no rounding to cents each month, so the totals can
differ from a lender's statement by a few cents.

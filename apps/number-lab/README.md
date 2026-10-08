# Number lab

A Wardian **module app**: 20 exact whole-number functions on 64-bit integers. There is no page.
Wardian draws a card for each function, with an input per parameter and a Run button.

Why a module: every function takes whole numbers and returns one. Wardian can show that without
help. And because every parameter and result is an `i64`, JavaScript gets a BigInt back. So
`fibonacci(92)` shows 7540113804746346429 to the last digit, where an ordinary JavaScript number
would round it after the 16th.

| File | What it is |
|---|---|
| `src/lib.rs` | The functions. Each `#[no_mangle] pub extern "C" fn` becomes a card in Wardian. Unit tests at the end. |
| `app.wasm` | The module Wardian runs. Built from `src/lib.rs`. |
| `app.json` | The title and the description in the app list. |
| `build.sh` | Rebuilds `app.wasm`. |

| Group | Functions |
|---|---|
| Primes | `is_prime`, `next_prime`, `prev_prime`, `nth_prime`, `prime_count` |
| Factors | `smallest_factor`, `largest_factor`, `divisor_count`, `euler_phi`, `gcd(a, b)`, `lcm(a, b)` |
| Modular | `mod_pow(base, exp, m)` |
| Sequences | `fibonacci`, `factorial`, `binomial(n, k)`, `collatz_steps`, `collatz_peak` |
| Digits | `digit_sum`, `reverse_digits`, `isqrt` |

How data moves: you type numbers in a card and press Run. Wardian first calls the function with
plain JavaScript numbers. The call fails, because the parameters are 64-bit, so Wardian calls it
again with BigInts. The module does the work and returns an `i64`, which arrives as a BigInt.
Inside, products use `u128`, so `mod_pow` and the prime test never overflow.

How it stays fast on big numbers:

- `is_prime` uses Miller–Rabin with the first twelve primes as witnesses. That test is exact for
  every number below 2^64, not "probably prime".
- The factor functions use Pollard's rho. `smallest_factor(9223371994482243049)` finds
  3037000493 in about 10 ms.
- `nth_prime` and `prime_count` use a sieve of odd numbers, one bit each.

Limits:

- Inputs and results are signed 64-bit: from −9223372036854775808 to 9223372036854775807.
  Type whole numbers only; `1e6` or `2.5` gives an error.
- **-1 means "no answer"**: the input is out of range, or the answer does not fit in 64 bits.
  Yes/no functions (`is_prime`) return 1 or 0.
- `fibonacci` takes 0 to 92 and `factorial` 0 to 20; the next ones are above the 64-bit limit.
- `nth_prime` takes 1 to 5,000,000 and `prime_count` 0 to 100,000,000. Each takes about
  a third of a second at the top of its range.
- Every parameter is `i64` on purpose. Wardian retries with BigInts for *all* parameters, so a
  function that mixed `i32` and `i64` parameters could not be called from the card.

Change it: edit `src/lib.rs`, run `cargo test`, run `./build.sh`, then `wardian check .`.

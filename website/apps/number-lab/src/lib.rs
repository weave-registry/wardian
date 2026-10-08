//! Number lab: exact whole-number functions on 64-bit integers.
//!
//! Every parameter and result is an `i64`, which JavaScript receives as a BigInt. So results stay
//! exact past 2^53, where ordinary JavaScript numbers start to round: fibonacci(92) is
//! 7540113804746346429, to the last digit.
//!
//! All parameters are `i64` on purpose. Wardian calls a function with plain numbers first and,
//! when that fails because a parameter is 64-bit, again with BigInts for every parameter. Mixing
//! `i32` and `i64` parameters in one function would break that second call.
//!
//! A result of -1 means "no answer": the input is out of range, or the answer does not fit in a
//! signed 64-bit integer (above 9223372036854775807). Yes/no functions return 1 or 0.
//! Products are done in `u128`, so nothing in here overflows silently.

const NONE: i64 = -1;

// ---------------------------------------------------------------- primes

/// (a * b) mod m without overflow.
fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

fn pow_mod(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut result = 1 % m;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            result = mul_mod(result, base, m);
        }
        base = mul_mod(base, base, m);
        exp >>= 1;
    }
    result
}

/// Miller–Rabin with the first twelve primes as witnesses: exact for every n below 2^64.
fn prime(n: u64) -> bool {
    const WITNESSES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    if n < 2 {
        return false;
    }
    for p in WITNESSES {
        if n % p == 0 {
            return n == p;
        }
    }
    let s = (n - 1).trailing_zeros();
    let d = (n - 1) >> s;
    'next: for a in WITNESSES {
        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mul_mod(x, x, n);
            if x == n - 1 {
                continue 'next;
            }
        }
        return false;
    }
    true
}

/// 1 if n is prime, else 0. Exact for every 64-bit n.
#[no_mangle]
pub extern "C" fn is_prime(n: i64) -> i64 {
    (n > 1 && prime(n as u64)) as i64
}

/// The smallest prime above n. -1 if it would not fit in an i64.
#[no_mangle]
pub extern "C" fn next_prime(n: i64) -> i64 {
    if n < 2 {
        return 2;
    }
    let mut c = n as u64 + 1;
    while c <= i64::MAX as u64 {
        if prime(c) {
            return c as i64;
        }
        c += 1;
    }
    NONE
}

/// The largest prime below n. -1 if n is 2 or less.
#[no_mangle]
pub extern "C" fn prev_prime(n: i64) -> i64 {
    let mut c = n - 1;
    while c >= 2 {
        if prime(c as u64) {
            return c;
        }
        c -= 1;
    }
    NONE
}

/// Marks the odd composites up to `limit`: bit i stands for the number 2i + 1.
fn odd_sieve(limit: u64) -> Vec<u64> {
    let bits = (limit / 2 + 1) as usize;
    let mut composite = vec![0u64; bits / 64 + 1];
    let mut i = 3u64;
    while i * i <= limit {
        if composite[(i / 2) as usize / 64] >> ((i / 2) % 64) & 1 == 0 {
            let mut j = i * i;
            while j <= limit {
                let k = (j / 2) as usize;
                composite[k / 64] |= 1 << (k % 64);
                j += 2 * i;
            }
        }
        i += 2;
    }
    composite
}

fn odd_is_prime(sieve: &[u64], n: u64) -> bool {
    let k = (n / 2) as usize;
    sieve[k / 64] >> (k % 64) & 1 == 0
}

/// How many primes are at most n (n up to 100,000,000). -1 outside that range.
#[no_mangle]
pub extern "C" fn prime_count(n: i64) -> i64 {
    if !(0..=100_000_000).contains(&n) {
        return NONE;
    }
    if n < 2 {
        return 0;
    }
    let n = n as u64;
    let sieve = odd_sieve(n);
    1 + (3..=n).step_by(2).filter(|&c| odd_is_prime(&sieve, c)).count() as i64
}

/// The n-th prime: nth_prime(1) = 2 (n from 1 to 5,000,000). -1 outside that range.
#[no_mangle]
pub extern "C" fn nth_prime(n: i64) -> i64 {
    if !(1..=5_000_000).contains(&n) {
        return NONE;
    }
    if n == 1 {
        return 2;
    }
    // Rosser's bound: the n-th prime is below n (ln n + ln ln n) for n >= 6.
    let f = n as f64;
    let limit = if n < 6 { 15 } else { (f * (f.ln() + f.ln().ln())) as u64 + 1 };
    let sieve = odd_sieve(limit);
    let mut seen = 1;
    let mut c = 3;
    while c <= limit {
        if odd_is_prime(&sieve, c) {
            seen += 1;
            if seen == n {
                return c as i64;
            }
        }
        c += 2;
    }
    NONE
}

// ---------------------------------------------------------------- factors

fn gcd_u(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Finds a factor of an odd composite n (Pollard's rho, Brent's variant).
fn rho(n: u64) -> u64 {
    let mut c = 1u64;
    loop {
        let f = |x: u64| (mul_mod(x, x, n) + c) % n;
        let (mut x, mut y, mut d) = (2u64, 2u64, 1u64);
        let mut q = 1u64;
        let mut ys = 2u64;
        let mut r = 1u64;
        while d == 1 {
            x = y;
            for _ in 0..r {
                y = f(y);
            }
            let mut k = 0;
            while k < r && d == 1 {
                ys = y;
                for _ in 0..(128.min(r - k)) {
                    y = f(y);
                    q = mul_mod(q, x.abs_diff(y), n);
                }
                d = gcd_u(q, n);
                k += 128;
            }
            r *= 2;
        }
        if d == n {
            // The batch overshot: step back one at a time.
            loop {
                ys = f(ys);
                d = gcd_u(x.abs_diff(ys), n);
                if d > 1 {
                    break;
                }
            }
        }
        if d != n {
            return d;
        }
        c += 1;
    }
}

/// Every prime factor of n, with repeats, smallest first.
fn factorize(n: u64) -> Vec<u64> {
    let mut out = Vec::new();
    let mut n = n;
    for p in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        while n % p == 0 {
            out.push(p);
            n /= p;
        }
    }
    let mut stack = vec![n];
    while let Some(m) = stack.pop() {
        if m == 1 {
            continue;
        }
        if prime(m) {
            out.push(m);
            continue;
        }
        let d = rho(m);
        stack.push(d);
        stack.push(m / d);
    }
    out.sort_unstable();
    out
}

/// The smallest prime that divides n (n from 2). -1 for n below 2.
#[no_mangle]
pub extern "C" fn smallest_factor(n: i64) -> i64 {
    if n < 2 {
        return NONE;
    }
    factorize(n as u64)[0] as i64
}

/// The largest prime that divides n (n from 2). -1 for n below 2.
#[no_mangle]
pub extern "C" fn largest_factor(n: i64) -> i64 {
    if n < 2 {
        return NONE;
    }
    *factorize(n as u64).last().unwrap() as i64
}

/// How many whole numbers divide n, 1 and n included (n from 1). -1 below 1.
#[no_mangle]
pub extern "C" fn divisor_count(n: i64) -> i64 {
    if n < 1 {
        return NONE;
    }
    let f = factorize(n as u64);
    let mut count = 1i64;
    let mut i = 0;
    while i < f.len() {
        let mut j = i;
        while j < f.len() && f[j] == f[i] {
            j += 1;
        }
        count *= (j - i + 1) as i64;
        i = j;
    }
    count
}

/// Euler's totient: how many numbers from 1 to n share no factor with n (n from 1). -1 below 1.
#[no_mangle]
pub extern "C" fn euler_phi(n: i64) -> i64 {
    if n < 1 {
        return NONE;
    }
    let mut f = factorize(n as u64);
    f.dedup();
    let mut phi = n as u64;
    for p in f {
        phi = phi / p * (p - 1);
    }
    phi as i64
}

/// Greatest common divisor of a and b (signs ignored). -1 only for gcd(-2^63, 0), which is 2^63.
#[no_mangle]
pub extern "C" fn gcd(a: i64, b: i64) -> i64 {
    let g = gcd_u(a.unsigned_abs(), b.unsigned_abs());
    i64::try_from(g).unwrap_or(NONE)
}

/// Least common multiple of a and b (signs ignored; 0 if either is 0). -1 if it does not fit.
#[no_mangle]
pub extern "C" fn lcm(a: i64, b: i64) -> i64 {
    let (a, b) = (a.unsigned_abs() as u128, b.unsigned_abs() as u128);
    if a == 0 || b == 0 {
        return 0;
    }
    let l = a / gcd_u(a as u64, b as u64) as u128 * b;
    i64::try_from(l).unwrap_or(NONE)
}

/// base^exp mod m, exactly (m from 1, exp from 0). A negative base counts up from m.
/// -1 for m below 1 or exp below 0.
#[no_mangle]
pub extern "C" fn mod_pow(base: i64, exp: i64, m: i64) -> i64 {
    if m < 1 || exp < 0 {
        return NONE;
    }
    pow_mod(base.rem_euclid(m) as u64, exp as u64, m as u64) as i64
}

// ---------------------------------------------------------------- sequences and digits

/// Steps for n to reach 1 under Collatz (halve if even, else 3n + 1). -1 for n below 1.
#[no_mangle]
pub extern "C" fn collatz_steps(n: i64) -> i64 {
    if n < 1 {
        return NONE;
    }
    // u128 because the path can climb far above the start.
    let mut x = n as u128;
    let mut steps = 0;
    while x != 1 {
        x = if x % 2 == 0 { x / 2 } else { 3 * x + 1 };
        steps += 1;
    }
    steps
}

/// The highest value n reaches on its way to 1 under Collatz. -1 for n below 1, or if it does not fit.
#[no_mangle]
pub extern "C" fn collatz_peak(n: i64) -> i64 {
    if n < 1 {
        return NONE;
    }
    let mut x = n as u128;
    let mut peak = x;
    while x != 1 {
        x = if x % 2 == 0 { x / 2 } else { 3 * x + 1 };
        peak = peak.max(x);
    }
    i64::try_from(peak).unwrap_or(NONE)
}

/// The sum of the decimal digits of n (sign ignored).
#[no_mangle]
pub extern "C" fn digit_sum(n: i64) -> i64 {
    let mut x = n.unsigned_abs();
    let mut sum = 0;
    while x > 0 {
        sum += (x % 10) as i64;
        x /= 10;
    }
    sum
}

/// The digits of n in reverse order (sign kept): 1234 → 4321. -1 if it does not fit.
#[no_mangle]
pub extern "C" fn reverse_digits(n: i64) -> i64 {
    let mut x = n.unsigned_abs();
    let mut r: u128 = 0;
    while x > 0 {
        r = r * 10 + (x % 10) as u128;
        x /= 10;
    }
    match i64::try_from(r) {
        Ok(r) if n < 0 => -r,
        Ok(r) => r,
        Err(_) => NONE,
    }
}

/// The whole square root of n, rounded down. -1 for n below 0.
#[no_mangle]
pub extern "C" fn isqrt(n: i64) -> i64 {
    if n < 0 {
        return NONE;
    }
    let n = n as u64;
    let mut r = (n as f64).sqrt() as u64;
    // The float guess can be off by one either way at this size; correct it exactly.
    while (r as u128) * (r as u128) > n as u128 {
        r -= 1;
    }
    while ((r + 1) as u128) * ((r + 1) as u128) <= n as u128 {
        r += 1;
    }
    r as i64
}

/// The n-th Fibonacci number: fibonacci(0) = 0, fibonacci(1) = 1 (n from 0 to 92).
/// -1 outside that range: fibonacci(93) is above the i64 limit.
#[no_mangle]
pub extern "C" fn fibonacci(n: i64) -> i64 {
    if !(0..=92).contains(&n) {
        return NONE;
    }
    let (mut a, mut b) = (0i64, 1i64);
    for _ in 0..n {
        (a, b) = (b, a.wrapping_add(b)); // b runs one step ahead and may wrap; a never does
    }
    a
}

/// n! = 1 × 2 × … × n (n from 0 to 20). -1 outside that range: 21! is above the i64 limit.
#[no_mangle]
pub extern "C" fn factorial(n: i64) -> i64 {
    if !(0..=20).contains(&n) {
        return NONE;
    }
    (1..=n).product()
}

/// "n choose k": the ways to pick k things from n. -1 for k outside 0..=n, or if it does not fit.
#[no_mangle]
pub extern "C" fn binomial(n: i64, k: i64) -> i64 {
    if n < 0 || k < 0 || k > n {
        return NONE;
    }
    let k = k.min(n - k) as u128;
    let n = n as u128;
    let mut r: u128 = 1;
    for i in 0..k {
        // r * (n - i) is divisible by i + 1, and stays below 2^127 while r fits in an i64.
        r = r * (n - i) / (i + 1);
        if r > i64::MAX as u128 {
            return NONE;
        }
    }
    r as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primes() {
        assert_eq!(is_prime(2), 1);
        assert_eq!(is_prime(1), 0);
        assert_eq!(is_prime(-7), 0);
        assert_eq!(is_prime(561), 0); // Carmichael number
        assert_eq!(is_prime(9223372036854775783), 1); // largest i64 prime
        assert_eq!(is_prime(3215031751), 0); // strong pseudoprime to 2, 3, 5, 7
        assert_eq!(next_prime(100), 101);
        assert_eq!(next_prime(9223372036854775783), -1);
        assert_eq!(prev_prime(100), 97);
        assert_eq!(prev_prime(2), -1);
        assert_eq!(nth_prime(1), 2);
        assert_eq!(nth_prime(6), 13);
        assert_eq!(nth_prime(10_000), 104_729);
        assert_eq!(nth_prime(1_000_000), 15_485_863);
        assert_eq!(prime_count(100), 25);
        assert_eq!(prime_count(1_000_000), 78_498);
        assert_eq!(prime_count(2), 1);
    }

    #[test]
    fn factors() {
        assert_eq!(smallest_factor(91), 7);
        assert_eq!(largest_factor(600851475143), 6857);
        let p: i64 = 4294967291; // largest prime below 2^32
        let q: i64 = 2147483647; // 2^31 - 1
        assert_eq!(smallest_factor(p * q), q);
        assert_eq!(largest_factor(p * q), p);
        assert_eq!(divisor_count(360), 24);
        assert_eq!(divisor_count(1), 1);
        assert_eq!(euler_phi(36), 12);
        assert_eq!(euler_phi(1), 1);
        assert_eq!(gcd(48, -18), 6);
        assert_eq!(gcd(i64::MIN, 0), -1);
        assert_eq!(lcm(4, 6), 12);
        assert_eq!(lcm(i64::MAX, i64::MAX - 1), -1);
        assert_eq!(mod_pow(2, 10, 1000), 24);
        assert_eq!(mod_pow(-2, 3, 5), 2);
        assert_eq!(mod_pow(4, 13, 497), 445);
        assert_eq!(mod_pow(2, 0, 1), 0);
        assert_eq!(mod_pow(i64::MAX, i64::MAX, i64::MAX - 1), 1);
    }

    #[test]
    fn sequences() {
        assert_eq!(collatz_steps(1), 0);
        assert_eq!(collatz_steps(27), 111);
        assert_eq!(collatz_peak(27), 9232);
        assert_eq!(digit_sum(-9875), 29);
        assert_eq!(digit_sum(i64::MIN), 89);
        assert_eq!(reverse_digits(-1230), -321);
        assert_eq!(reverse_digits(i64::MAX), 7085774586302733229);
        assert_eq!(reverse_digits(1999999999999999999), -1);
        assert_eq!(isqrt(i64::MAX), 3037000499);
        assert_eq!(isqrt(15), 3);
        assert_eq!(fibonacci(0), 0);
        assert_eq!(fibonacci(92), 7540113804746346429);
        assert_eq!(fibonacci(93), -1);
        assert_eq!(factorial(0), 1);
        assert_eq!(factorial(20), 2432902008176640000);
        assert_eq!(factorial(21), -1);
        assert_eq!(binomial(52, 5), 2598960);
        assert_eq!(binomial(66, 33), 7219428434016265740);
        assert_eq!(binomial(68, 34), -1);
    }
}

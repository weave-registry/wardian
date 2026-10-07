//! A Wardian module app: plain functions of numbers.
//!
//! The host lists every exported function with one input per parameter.
//! Parameters and results must be numbers: i32, i64, f32 or f64.
//! An i64 becomes a JavaScript BigInt, so fib() works past 2^53.

/// Adds two whole numbers.
#[no_mangle]
pub extern "C" fn add(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

/// The n-th Fibonacci number (fib(90) still fits in an i64).
#[no_mangle]
pub extern "C" fn fib(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n.min(93) {
        (a, b) = (b, a.wrapping_add(b));
    }
    a
}

/// Degrees Celsius to Fahrenheit, to show a float.
#[no_mangle]
pub extern "C" fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

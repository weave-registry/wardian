//! Loan amortization in WebAssembly.
//!
//! `schedule` writes one row per month into a buffer the caller reserved with `alloc`:
//! three f64 values per row — interest paid, principal paid, balance left. It returns the number
//! of months until the loan is paid off. Buffers are 8-byte aligned so JavaScript can read them
//! as a Float64Array.

use std::alloc::{alloc as raw_alloc, dealloc as raw_dealloc, Layout};

const ALIGN: usize = 8;

/// Reserves `len` bytes, 8-byte aligned, and returns where they start.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::<f64>::dangling().as_ptr().cast();
    }
    unsafe { raw_alloc(Layout::from_size_align_unchecked(len, ALIGN)) }
}

/// Gives back space from `alloc`.
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if len != 0 {
        unsafe { raw_dealloc(ptr, Layout::from_size_align_unchecked(len, ALIGN)) }
    }
}

/// The fixed monthly payment that repays `principal` over `years` at `rate_pct` a year.
#[no_mangle]
pub extern "C" fn payment(principal: f64, rate_pct: f64, years: u32) -> f64 {
    let n = f64::from(years.max(1) * 12);
    let r = rate_pct / 1200.0;
    if r.abs() < 1e-12 {
        principal / n
    } else {
        principal * r / (1.0 - (1.0 + r).powf(-n))
    }
}

/// Fills `out` with up to `max_rows` monthly rows (interest, principal, balance) when paying the
/// regular payment plus `extra` each month. Returns how many months it takes to reach zero.
#[no_mangle]
pub extern "C" fn schedule(principal: f64, rate_pct: f64, years: u32, extra: f64, out: *mut f64, max_rows: u32) -> u32 {
    let rows = unsafe { std::slice::from_raw_parts_mut(out, max_rows as usize * 3) };
    let pay = payment(principal, rate_pct, years) + extra.max(0.0);
    let r = rate_pct / 1200.0;
    let mut balance = principal;
    let mut month = 0usize;
    while balance > 0.005 && month < max_rows as usize {
        let interest = balance * r;
        let principal_paid = (pay - interest).min(balance);
        if principal_paid <= 0.0 {
            break; // the payment does not even cover the interest
        }
        balance -= principal_paid;
        rows[month * 3..month * 3 + 3].copy_from_slice(&[interest, principal_paid, balance.max(0.0)]);
        month += 1;
    }
    month as u32
}

//! Renders the Mandelbrot set into an RGBA pixel buffer.
//!
//! The page owns the buffer: it asks for space with `alloc`, passes the address to `render`,
//! then draws the bytes on a canvas. WebAssembly fills width × height × 4 bytes (red, green,
//! blue, alpha) in one call, which is far faster than the same loop in JavaScript.

use std::alloc::{alloc as raw_alloc, dealloc as raw_dealloc, Layout};

/// Reserves `len` bytes in the module's memory and returns where they start.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::dangling().as_ptr();
    }
    unsafe { raw_alloc(Layout::from_size_align_unchecked(len, 1)) }
}

/// Gives back space from `alloc`.
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if len != 0 {
        unsafe { raw_dealloc(ptr, Layout::from_size_align_unchecked(len, 1)) }
    }
}

/// A smooth colour for a point that escaped after `n` steps (between 0 and 1 of the way through
/// the palette). Cosine palettes give soft bands with no hard edges.
fn colour(t: f64) -> [u8; 3] {
    let channel = |phase: f64| {
        let v = 0.5 + 0.5 * (std::f64::consts::TAU * (t + phase)).cos();
        (v * 255.0) as u8
    };
    [channel(0.00), channel(0.10), channel(0.20)]
}

/// Draws the view centred on (`cx`, `cy`) where one pixel is `scale` units wide, into the
/// `width` × `height` RGBA buffer at `out`. Points inside the set are dark. Returns how many
/// pixels are inside the set, which the page shows as a hint to raise the iteration limit.
#[no_mangle]
pub extern "C" fn render(out: *mut u8, width: u32, height: u32, cx: f64, cy: f64, scale: f64, max_iter: u32) -> u32 {
    let (w, h) = (width as usize, height as usize);
    let pixels = unsafe { std::slice::from_raw_parts_mut(out, w * h * 4) };
    let max_iter = max_iter.max(1);
    let mut inside = 0u32;
    for py in 0..h {
        let y0 = cy + (py as f64 - h as f64 / 2.0) * scale;
        for px in 0..w {
            let x0 = cx + (px as f64 - w as f64 / 2.0) * scale;
            // Skip the main cardioid and the period-2 bulb: they are inside, and the slowest
            // points to iterate.
            let q = (x0 - 0.25) * (x0 - 0.25) + y0 * y0;
            let in_bulb = q * (q + (x0 - 0.25)) <= 0.25 * y0 * y0 || (x0 + 1.0) * (x0 + 1.0) + y0 * y0 <= 0.0625;
            let (mut x, mut y, mut n) = (0.0f64, 0.0f64, 0u32);
            if !in_bulb {
                while n < max_iter && x * x + y * y <= 256.0 {
                    let xt = x * x - y * y + x0;
                    y = 2.0 * x * y + y0;
                    x = xt;
                    n += 1;
                }
            }
            let i = (py * w + px) * 4;
            if in_bulb || n >= max_iter {
                pixels[i..i + 4].copy_from_slice(&[8, 10, 22, 255]);
                inside += 1;
            } else {
                // Smooth iteration count: removes the stepped bands of plain counting.
                let smooth = n as f64 + 1.0 - ((x * x + y * y).ln() / 2.0).ln() / std::f64::consts::LN_2;
                let [r, g, b] = colour(0.62 + smooth * 0.022);
                pixels[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    inside
}

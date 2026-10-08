//! Photo filters that work in place on an RGBA pixel buffer.
//!
//! The page owns the buffer: it asks for `width × height × 4` bytes with `alloc`, copies the
//! picture in, and calls a filter with the address. Each filter changes the bytes where they lie
//! (red, green, blue, alpha, one byte each), so the page reads the result from the same place.
//! Alpha is never changed. Filters that need neighbouring pixels (blur, sharpen, edges) copy what
//! they need into a scratch buffer first, so the answer never depends on the order of the loop.

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

fn pixels<'a>(ptr: *mut u8, w: u32, h: u32) -> &'a mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(ptr, w as usize * h as usize * 4) }
}

fn clamp(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Perceived brightness of a pixel (ITU-R BT.601 weights), 0 to 255.
fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

/// Applies `f` to the red, green and blue of every pixel.
fn per_pixel(ptr: *mut u8, w: u32, h: u32, f: impl Fn(u8, u8, u8) -> [u8; 3]) {
    for px in pixels(ptr, w, h).chunks_exact_mut(4) {
        let [r, g, b] = f(px[0], px[1], px[2]);
        px[0] = r;
        px[1] = g;
        px[2] = b;
    }
}

#[no_mangle]
pub extern "C" fn grayscale(ptr: *mut u8, w: u32, h: u32) {
    per_pixel(ptr, w, h, |r, g, b| {
        let y = clamp(luma(r, g, b));
        [y, y, y]
    });
}

/// The classic warm brown tone of old photographs.
#[no_mangle]
pub extern "C" fn sepia(ptr: *mut u8, w: u32, h: u32) {
    per_pixel(ptr, w, h, |r, g, b| {
        let (r, g, b) = (r as f32, g as f32, b as f32);
        [
            clamp(0.393 * r + 0.769 * g + 0.189 * b),
            clamp(0.349 * r + 0.686 * g + 0.168 * b),
            clamp(0.272 * r + 0.534 * g + 0.131 * b),
        ]
    });
}

#[no_mangle]
pub extern "C" fn invert(ptr: *mut u8, w: u32, h: u32) {
    per_pixel(ptr, w, h, |r, g, b| [255 - r, 255 - g, 255 - b]);
}

/// `brightness` and `contrast` run from -100 to 100; 0 leaves the picture as it is.
/// Brightness adds up to ±255 to each channel; contrast stretches or squeezes around mid-grey.
#[no_mangle]
pub extern "C" fn brightness_contrast(ptr: *mut u8, w: u32, h: u32, brightness: f32, contrast: f32) {
    let add = brightness.clamp(-100.0, 100.0) * 2.55;
    let c = contrast.clamp(-100.0, 100.0) * 2.55;
    let factor = (259.0 * (c + 255.0)) / (255.0 * (259.0 - c));
    // A lookup table: 256 sums once, instead of one per channel per pixel.
    let mut table = [0u8; 256];
    for (v, slot) in table.iter_mut().enumerate() {
        *slot = clamp(factor * (v as f32 + add - 128.0) + 128.0);
    }
    per_pixel(ptr, w, h, |r, g, b| [table[r as usize], table[g as usize], table[b as usize]]);
}

/// `levels` colours per channel (2 to 32): flat areas of colour like a screen print.
#[no_mangle]
pub extern "C" fn posterize(ptr: *mut u8, w: u32, h: u32, levels: u32) {
    let n = levels.clamp(2, 32) as f32 - 1.0;
    let mut table = [0u8; 256];
    for (v, slot) in table.iter_mut().enumerate() {
        *slot = clamp((v as f32 / 255.0 * n).round() / n * 255.0);
    }
    per_pixel(ptr, w, h, |r, g, b| [table[r as usize], table[g as usize], table[b as usize]]);
}

/// Black or white only: a pixel brighter than `level` (0 to 255) turns white.
#[no_mangle]
pub extern "C" fn threshold(ptr: *mut u8, w: u32, h: u32, level: u32) {
    let level = level.min(255) as f32;
    per_pixel(ptr, w, h, |r, g, b| {
        let v = if luma(r, g, b) > level { 255 } else { 0 };
        [v, v, v]
    });
}

/// One pass of a box blur along a line of `n` pixels, `step` bytes apart, starting at `start`.
/// A running sum keeps the cost the same for any radius. Edge pixels repeat beyond the border.
fn box_line(src: &[u8], dst: &mut [u8], start: usize, n: usize, step: usize, r: usize) {
    let span = (2 * r + 1) as u32;
    let at = |i: isize| start + (i.clamp(0, n as isize - 1) as usize) * step;
    for c in 0..3 {
        let mut sum: u32 = 0;
        for i in -(r as isize)..=(r as isize) {
            sum += src[at(i) + c] as u32;
        }
        for i in 0..n {
            dst[start + i * step + c] = ((sum + span / 2) / span) as u8;
            sum += src[at(i as isize + r as isize + 1) + c] as u32;
            sum -= src[at(i as isize - r as isize) + c] as u32;
        }
    }
}

/// A box blur of radius `r` across the whole buffer: across every row, then down every column.
fn box_pass(px: &mut [u8], scratch: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 {
        return;
    }
    scratch.copy_from_slice(px);
    for y in 0..h {
        box_line(scratch, px, y * w * 4, w, 4, r);
    }
    scratch.copy_from_slice(px);
    for x in 0..w {
        box_line(scratch, px, x * 4, h, w * 4, r);
    }
}

#[no_mangle]
pub extern "C" fn box_blur(ptr: *mut u8, w: u32, h: u32, radius: u32) {
    let px = pixels(ptr, w, h);
    let mut scratch = vec![0u8; px.len()];
    box_pass(px, &mut scratch, w as usize, h as usize, radius.min(200) as usize);
}

/// Three box blurs in a row look almost exactly like a gaussian blur and cost the same for any
/// size. The radii come from "Fast almost-Gaussian filtering" (Kovesi, 2010).
fn gaussian(px: &mut [u8], w: usize, h: usize, sigma: f32) {
    if sigma < 0.3 {
        return;
    }
    let mut scratch = vec![0u8; px.len()];
    let n = 3.0f32;
    let ideal = ((12.0 * sigma * sigma / n) + 1.0).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f32 - 4.0 * n * wl as f32 - 3.0 * n) / (-4.0 * wl as f32 - 4.0)).round() as i32;
    for i in 0..3 {
        let size = if i < m { wl } else { wu };
        box_pass(px, &mut scratch, w, h, ((size - 1) / 2).max(0) as usize);
    }
}

/// `sigma` is the blur's spread in pixels (0.3 to 100).
#[no_mangle]
pub extern "C" fn gaussian_blur(ptr: *mut u8, w: u32, h: u32, sigma: f32) {
    gaussian(pixels(ptr, w, h), w as usize, h as usize, sigma.min(100.0));
}

/// Unsharp mask: adds back `amount` (0 to 5) times the difference between the picture and a
/// blurred copy of it, which makes edges crisper. `sigma` sets how wide an edge counts.
#[no_mangle]
pub extern "C" fn sharpen(ptr: *mut u8, w: u32, h: u32, amount: f32, sigma: f32) {
    let px = pixels(ptr, w, h);
    let mut blurred = px.to_vec();
    gaussian(&mut blurred, w as usize, h as usize, sigma.clamp(0.3, 20.0));
    let amount = amount.clamp(0.0, 5.0);
    for (i, (p, b)) in px.iter_mut().zip(blurred.iter()).enumerate() {
        if i % 4 != 3 {
            *p = clamp(*p as f32 + amount * (*p as f32 - *b as f32));
        }
    }
}

/// Sobel edge detection: bright lines where brightness changes fast, black where it is flat.
#[no_mangle]
pub extern "C" fn sobel(ptr: *mut u8, w: u32, h: u32) {
    let (w, h) = (w as usize, h as usize);
    let px = pixels(ptr, w as u32, h as u32);
    let grey: Vec<f32> = px.chunks_exact(4).map(|p| luma(p[0], p[1], p[2])).collect();
    let at = |x: isize, y: isize| grey[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1) - 2.0 * at(x - 1, y) - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1) - 2.0 * at(x, y - 1) - at(x + 1, y - 1);
            // The largest possible magnitude is about 1443; scale so typical edges show clearly.
            let v = clamp((gx * gx + gy * gy).sqrt() * 0.5);
            let i = (y as usize * w + x as usize) * 4;
            px[i] = v;
            px[i + 1] = v;
            px[i + 2] = v;
        }
    }
}

/// Counts of each value, 0 to 255, for red, then green, then blue: 768 numbers.
static mut HISTOGRAM: [u32; 768] = [0; 768];

/// Where the 768 histogram counts live. The page reads them as a Uint32Array after `histogram`.
#[no_mangle]
pub extern "C" fn histogram_ptr() -> *const u32 {
    std::ptr::addr_of!(HISTOGRAM) as *const u32
}

/// Counts every red, green and blue value in the picture into the histogram. Returns the
/// largest count, so the page can scale its chart without a second pass.
#[no_mangle]
pub extern "C" fn histogram(ptr: *mut u8, w: u32, h: u32) -> u32 {
    let hist = unsafe { &mut *std::ptr::addr_of_mut!(HISTOGRAM) };
    hist.fill(0);
    for px in pixels(ptr, w, h).chunks_exact(4) {
        hist[px[0] as usize] += 1;
        hist[256 + px[1] as usize] += 1;
        hist[512 + px[2] as usize] += 1;
    }
    hist.iter().copied().max().unwrap_or(0)
}

//! Conway's Game of Life, and its cousins, on a grid whose edges wrap around (a torus).
//!
//! The module keeps the whole world in its own memory: two grids of one byte per cell (1 alive,
//! 0 dead). `step(n)` reads one grid and writes the next generation into the other, then swaps
//! them, `n` times in one call. A third grid, `heat`, remembers how recently each cell died, so
//! `render` can draw a fading trail behind moving patterns.
//!
//! The page asks for the size with `init`, sets cells with `set_cell`, picks a rule with
//! `set_rule`, and draws with `render`, which writes width × height RGBA pixels into a buffer
//! the page reserved with `alloc`.

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

struct World {
    w: usize,
    h: usize,
    cells: Vec<u8>,
    next: Vec<u8>,
    heat: Vec<u8>,
    /// Bit `n` set: a dead cell with `n` live neighbours is born.
    birth: u16,
    /// Bit `n` set: a live cell with `n` live neighbours stays alive.
    survive: u16,
    seed: u32,
}

static mut WORLD: World = World { w: 0, h: 0, cells: Vec::new(), next: Vec::new(), heat: Vec::new(), birth: 1 << 3, survive: (1 << 2) | (1 << 3), seed: 0x9E37_79B9 };

fn world() -> &'static mut World {
    // The page calls one function at a time on one thread, so one shared world is safe.
    unsafe { &mut *std::ptr::addr_of_mut!(WORLD) }
}

/// Makes an empty world `width` × `height` cells (each at least 3, at most 2000).
#[no_mangle]
pub extern "C" fn init(width: u32, height: u32) {
    let w = world();
    w.w = width.clamp(3, 2000) as usize;
    w.h = height.clamp(3, 2000) as usize;
    let n = w.w * w.h;
    w.cells = vec![0; n];
    w.next = vec![0; n];
    w.heat = vec![0; n];
}

/// Sets the rule from two bit masks: bit `n` of `birth` means "born with n neighbours", bit `n` of
/// `survive` means "survives with n neighbours". Conway's B3/S23 is birth 0b1000, survive 0b1100.
#[no_mangle]
pub extern "C" fn set_rule(birth: u32, survive: u32) {
    let w = world();
    w.birth = (birth & 0x1FF) as u16;
    w.survive = (survive & 0x1FF) as u16;
}

fn index(w: &World, x: i32, y: i32) -> usize {
    let x = x.rem_euclid(w.w as i32) as usize;
    let y = y.rem_euclid(w.h as i32) as usize;
    y * w.w + x
}

/// Sets one cell alive (1) or dead (0). Coordinates wrap around the edges.
#[no_mangle]
pub extern "C" fn set_cell(x: i32, y: i32, alive: u32) {
    let w = world();
    if w.cells.is_empty() {
        return;
    }
    let i = index(w, x, y);
    w.cells[i] = (alive != 0) as u8;
    if alive == 0 {
        w.heat[i] = 0;
    }
}

/// 1 if the cell is alive, else 0.
#[no_mangle]
pub extern "C" fn get_cell(x: i32, y: i32) -> u32 {
    let w = world();
    if w.cells.is_empty() {
        return 0;
    }
    w.cells[index(w, x, y)] as u32
}

/// Kills every cell and clears the trails.
#[no_mangle]
pub extern "C" fn clear() {
    let w = world();
    w.cells.fill(0);
    w.heat.fill(0);
}

/// Fills the world at random: each cell is alive with chance `density` (0 to 1). Returns the
/// population.
#[no_mangle]
pub extern "C" fn randomize(density: f64, seed: u32) -> u32 {
    let w = world();
    if seed != 0 {
        w.seed = seed;
    }
    let limit = (density.clamp(0.0, 1.0) * u32::MAX as f64) as u32;
    let mut s = w.seed;
    for (c, h) in w.cells.iter_mut().zip(w.heat.iter_mut()) {
        // xorshift32: small, fast, and good enough to scatter cells.
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        *c = (s < limit) as u8;
        *h = 0;
    }
    w.seed = s;
    population()
}

/// Advances the world `n` generations in one call and returns the population after the last.
#[no_mangle]
pub extern "C" fn step(n: u32) -> u32 {
    let w = world();
    let (width, height) = (w.w, w.h);
    if width == 0 {
        return 0;
    }
    for _ in 0..n {
        for y in 0..height {
            let up = if y == 0 { height - 1 } else { y - 1 } * width;
            let down = if y + 1 == height { 0 } else { y + 1 } * width;
            let row = y * width;
            for x in 0..width {
                let left = if x == 0 { width - 1 } else { x - 1 };
                let right = if x + 1 == width { 0 } else { x + 1 };
                let c = &w.cells;
                let count = c[up + left] + c[up + x] + c[up + right]
                    + c[row + left] + c[row + right]
                    + c[down + left] + c[down + x] + c[down + right];
                let alive = c[row + x] != 0;
                let mask = if alive { w.survive } else { w.birth };
                w.next[row + x] = ((mask >> count) & 1) as u8;
            }
        }
        std::mem::swap(&mut w.cells, &mut w.next);
        // A cell that is alive is hot; a dead one cools a little each generation.
        for (c, h) in w.cells.iter().zip(w.heat.iter_mut()) {
            *h = if *c != 0 { 255 } else { h.saturating_sub(24) };
        }
    }
    population()
}

/// How many cells are alive.
#[no_mangle]
pub extern "C" fn population() -> u32 {
    world().cells.iter().map(|&c| c as u32).sum()
}

fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32, ((c >> 8) & 255) as f32, (c & 255) as f32]
}

/// Writes one RGBA pixel per cell into `out` (width × height × 4 bytes). Colours are 0xRRGGBB:
/// `live` for live cells, `dead` for the background. With `trail` set, cells that died recently
/// fade from the `glow` colour back to the background.
#[no_mangle]
pub extern "C" fn render(out: *mut u8, live: u32, dead: u32, glow: u32, trail: u32) {
    let w = world();
    let px = unsafe { std::slice::from_raw_parts_mut(out, w.w * w.h * 4) };
    let (l, d, g) = (rgb(live), rgb(dead), rgb(glow));
    for ((p, &c), &h) in px.chunks_exact_mut(4).zip(w.cells.iter()).zip(w.heat.iter()) {
        let colour = if c != 0 {
            l
        } else if trail != 0 && h > 0 {
            let t = h as f32 / 255.0 * 0.55;
            [d[0] + (g[0] - d[0]) * t, d[1] + (g[1] - d[1]) * t, d[2] + (g[2] - d[2]) * t]
        } else {
            d
        };
        p[0] = colour[0] as u8;
        p[1] = colour[1] as u8;
        p[2] = colour[2] as u8;
        p[3] = 255;
    }
}

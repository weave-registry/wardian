//! Text in, text out, through the module's memory.
//!
//! WebAssembly functions only take numbers, so text travels like this:
//!   1. JavaScript asks for space:      ptr = alloc(len)
//!   2. JavaScript copies the bytes in:  new Uint8Array(memory.buffer, ptr, len).set(bytes)
//!   3. JavaScript calls a function:     upper(ptr, len), or words(ptr, len)
//!   4. JavaScript reads the bytes back, then frees the space: dealloc(ptr, len)

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

fn bytes<'a>(ptr: *mut u8, len: usize) -> &'a mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(ptr, len) }
}

/// Makes ASCII letters upper case, in place.
#[no_mangle]
pub extern "C" fn upper(ptr: *mut u8, len: usize) {
    bytes(ptr, len).make_ascii_uppercase();
}

/// Counts the words: runs of characters between spaces, tabs or new lines.
#[no_mangle]
pub extern "C" fn words(ptr: *mut u8, len: usize) -> u32 {
    let text = bytes(ptr, len);
    text.split(|b| b.is_ascii_whitespace()).filter(|w| !w.is_empty()).count() as u32
}

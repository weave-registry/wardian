# {{name}}

A Wardian **module app**: a WebAssembly module whose functions take and return numbers.
Wardian lists each exported function with an input box for each parameter.

| File | What it is |
|---|---|
| `app.wasm` | The module Wardian runs. Built from `src/lib.rs`. |
| `app.json` | Title and description for the app list. |
| `src/lib.rs` | The source. Each `#[no_mangle] pub extern "C" fn` becomes a function in Wardian. |
| `build.sh` | Rebuilds `app.wasm` after you change the source. |

Change it:

1. Edit `src/lib.rs`. Use only number types for parameters and results: `i32`, `i64`, `u32`, `u64`, `f32`, `f64`.
2. Run `./build.sh`.
3. Run `wardian check .`, then reload the app in Wardian.

Need text, arrays or a real interface? Start from the page template instead: `wardian new page <name>`.

Wardian draws a module's page itself, with the Wardian components and Arrange, so this package
needs no `ui/` folder. For your own interface, use a page app (`wardian new page`).

# {{name}}

A rustle **module app**: a WebAssembly module whose functions take and return numbers.
rustle lists each exported function with an input box for each parameter.

| File | What it is |
|---|---|
| `app.wasm` | The module rustle runs. Built from `src/lib.rs`. |
| `app.json` | Title and description for the app list. |
| `src/lib.rs` | The source. Each `#[no_mangle] pub extern "C" fn` becomes a function in rustle. |
| `build.sh` | Rebuilds `app.wasm` after you change the source. |

Change it:

1. Edit `src/lib.rs`. Use only number types for parameters and results: `i32`, `i64`, `u32`, `u64`, `f32`, `f64`.
2. Run `./build.sh`.
3. Run `rustle check .`, then reload the app in rustle.

Need text, arrays or a real interface? Start from the page template instead: `rustle new page <name>`.

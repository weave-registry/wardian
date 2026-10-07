# {{name}}

A Wardian **page app**: a WebAssembly module plus a page that drives it.
Use a page whenever the module works on text, arrays or JSON, or needs a real interface.

| File | What it is |
|---|---|
| `index.html` | The page Wardian shows. Named in `app.json`. |
| `ui/` | The Wardian component library: theme, button, field, card and Arrange. Your copy: change it freely. `wardian add <component> .` adds more. |
| `app.js` | Loads `app.wasm` and passes text through the module's memory. |
| `app.wasm` | The module. Built from `src/lib.rs`. |
| `src/lib.rs` | The source: `alloc`, `dealloc`, `upper`, `words`. |
| `build.sh` | Rebuilds `app.wasm` after you change the source. |

How text gets in and out: WebAssembly functions take only numbers. So `app.js` asks the module
for space (`alloc`), copies the bytes in, calls the function with the address and length, reads
the bytes back, and frees the space (`dealloc`).

The page runs sandboxed: it cannot use `localStorage`, cookies, or the host's pages.
It can load files from its own package with relative URLs.

Arrange: each part marked `data-panel` inside a `data-arrange-column` is a panel the viewer may
reorder, move to the other column or hide, for themselves only. `ui/arrange.js` adds the Arrange
button. Mark your own parts the same way, and give each a `data-panel-label`.

Change it: edit `src/lib.rs` and `app.js`, run `./build.sh`, then `wardian check .`.

# Mandelbrot explorer

A Wardian **page app**. WebAssembly renders the Mandelbrot set into an RGBA pixel buffer, and the
page draws that buffer on a canvas. Click to zoom in ×4 on a point; shift-click or right-click
to zoom out; drag the **Detail** slider for more iterations at deep zoom; **Save image** downloads
a PNG.

| File | What it is |
|---|---|
| `index.html` | The page: controls, canvas and status line. |
| `app.js` | Sizes the canvas, asks the module to render, puts the pixels on screen, handles zoom. |
| `src/lib.rs` | `render(out, width, height, cx, cy, scale, max_iter)` plus `alloc` / `dealloc`. |
| `app.wasm` | Built from `src/lib.rs` by `./build.sh`. |

Why WebAssembly: a 1600 × 1066 view at 400 iterations is hundreds of millions of steps. One call
into the module fills the whole buffer; no per-pixel calls cross into JavaScript.

How the pixels travel: the page reserves `width × height × 4` bytes with `alloc`, passes the
address to `render`, then wraps the same bytes in an `ImageData`. The buffer is reused until the
canvas size changes. Nothing is copied twice.

Limits: coordinates are 64-bit floats, so detail runs out near ×10¹³ zoom; the status line says when.

The page uses the Wardian component library in `ui/` (theme, button, field, card) and Arrange
(`ui/arrange.js`): the controls, the position read-out, the help and the picture are panels
(`data-panel`) the viewer may reorder, move between the two columns, or hide.

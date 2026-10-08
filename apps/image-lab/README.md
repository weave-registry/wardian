# Image lab

A Wardian **page app**. Open a photo, and WebAssembly filters its pixels: grayscale, sepia, invert,
brightness and contrast, gaussian or box blur, sharpen, edge detect, posterize and threshold. Drag
the line across the picture to compare before and after. The histogram updates after every
change. **Undo**, **Reset** and **Save image** (a PNG download) are in the History panel.

**Your picture never leaves this computer.** The page sends nothing anywhere: the file goes from
your disk into the page's memory and nowhere else, and the only way out is the PNG you choose to
save. The browser also enforces it: Wardian lets a page app request only its own package's files
(ADR-2610081003), so the page could not upload the picture even if its code tried.

Why a page: the app moves whole pictures through memory and needs a canvas, sliders and a
histogram. A module app passes only numbers. It is a page and not a suite because it has one job,
editing one picture, and every panel works on the same pixels.

| File | What it is |
|---|---|
| `index.html` | The page: the open, filter, adjust and history panels, the picture and the histogram. |
| `app.js` | Opens the file, keeps the original and the undo steps, calls the filters, draws the result. |
| `src/lib.rs` | The filters, `histogram`, and `alloc` / `dealloc`. |
| `app.wasm` | Built from `src/lib.rs` by `./build.sh`. |
| `ui/` | The Wardian component library: theme, button, field, card, badge, switch, toast and Arrange. |

How the pixels travel:

1. The browser decodes the file into RGBA bytes on a canvas.
2. `app.js` reserves `width × height × 4` bytes in the module with `alloc`. It reuses that buffer
   until a picture of a different size opens.
3. Each filter copies the current picture into the buffer, then calls, for example,
   `gaussian_blur(ptr, width, height, sigma)`. The module changes the bytes in place.
4. `histogram(ptr, width, height)` counts the red, green and blue values into 768 numbers in the
   module's memory. `histogram_ptr()` tells the page where they are.
5. The page reads `memory.buffer` again after each call, because a call can grow the memory and
   replace the buffer. Then it draws the bytes on the canvas.

A slider shows a preview at once and changes nothing until you press **Apply**. **Discard** drops
the preview. "Before" is always the picture as you opened it.

The blurs cost the same for any radius. A box blur keeps a running sum along each row and column.
A gaussian blur is three box blurs in a row, which looks almost the same. Sharpen is an unsharp
mask: it adds back the difference between the picture and a blurred copy.

Limits:

- A picture larger than 12 megapixels opens scaled down, so filters stay quick. The page says so.
- Undo keeps up to about 200 MB of steps. Older steps drop off first.
- The saved PNG has no EXIF data (camera, date, location). Some people will like that.
- The page cannot keep anything between visits. Save the PNG to keep your work.

The panels are Arrange panels (`data-panel`): each viewer may reorder them, move them between the
two columns, or hide them.

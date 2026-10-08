<!-- A copy of a Wardian docs page, written by `wardian skills`. Do not edit it here:
     `wardian skills --force` replaces it. Links that start with /docs/ are pages of a running Wardian. -->

# Page apps in depth

A page app is one tool with one job and your own interface. It is an HTML page in a sandboxed
frame. It may use WebAssembly for the hard part, or plain JavaScript for all of it.

Start one with `wardian new page apps/<name>`. The examples [`text-tools`](/docs/examples#text-tools),
[`image-lab`](/docs/examples#image-lab), [`life`](/docs/examples#life),
[`mandelbrot`](/docs/examples#mandelbrot) and [`focus-timer`](/docs/examples#focus-timer) are page apps.

## How the page is found

The host shows `page` from `app.json`. Without it, the host uses the first of these that exists:
`index.html`, `demo/index.html`, `www/index.html`, `web/index.html`.

```json
{ "format": 1, "title": "Text tools", "description": "Counts, hashes and changes text.", "page": "index.html" }
```

The host serves the package's folder tree as it is, at `/apps/<name>/<path>`. So the page loads its
own files with relative links: `./app.wasm`, `../pkg/app.js`, `./ui/theme.css`.

A page app still needs an `app.wasm` at its top, because that file marks the folder as a module
app. A page that uses no WebAssembly can keep the empty module that `wardian new page` writes.

## The sandbox

Wardian sends every HTML and SVG file of a page app with a policy that starts
`sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads`, and then allows
requests only to the package's own folder, `/apps/<name>/`, and to Wardian's page library, `/sdk/`
(ADR-2610081003). The browser gives the page a unique, throwaway origin.

| The page can | The page cannot |
|---|---|
| run scripts, use forms and dialogs | read Wardian's pages or settings |
| start downloads | use cookies, `localStorage`, `sessionStorage` or IndexedDB |
| fetch, and load scripts, styles, images and `.wasm`, from its own package | request any other address: the internet, Wardian's API, other apps |
| load `/sdk/wardian.js` and Google Fonts | load anything else from outside |
| open files the user picks (`<input type="file">`, drag and drop) | read any file the user did not pick |

So a page app must ship every file it uses. A library from a CDN does not load: copy it into the
package. To keep data, let the user download it and load it again, or build a suite, whose parts get
`storage` and `db` from the kernel.

`tests/run-page-sandbox-e2e.sh` proves these limits against a hostile page. Three routes stay open,
as in a suite: a pop-up after a click, navigating the frame away, and WebRTC
([Security model](/docs/security#three-routes-stay-open)).

## Moving data through WebAssembly memory

A WebAssembly function takes and returns numbers only. To pass text, an image or a list, copy the
bytes into the module's memory and pass the address and the length.

1. Ask the module for space: `ptr = alloc(len)`.
2. Copy the bytes in at `ptr`.
3. Call the function with the address and the length.
4. Read the result back.
5. Free the space: `dealloc(ptr, len)`.

The page template does this in `withText`:

```js
function withText(text, fn) {
  const bytes = new TextEncoder().encode(text);
  const ptr = wasm.alloc(bytes.length);
  try {
    new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    const result = fn(ptr, bytes.length);
    // Read memory.buffer again: a call can grow memory and replace the buffer.
    const back = new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, ptr, bytes.length));
    return [result, back];
  } finally {
    wasm.dealloc(ptr, bytes.length);
  }
}
```

And on the Rust side, from the template's `src/lib.rs`:

```rust
use std::alloc::{alloc as raw_alloc, dealloc as raw_dealloc, Layout};

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::dangling().as_ptr();
    }
    unsafe { raw_alloc(Layout::from_size_align_unchecked(len, 1)) }
}

#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if len != 0 {
        unsafe { raw_dealloc(ptr, Layout::from_size_align_unchecked(len, 1)) }
    }
}
```

The alignment is 1, which suits bytes. For numbers, use the alignment of the type:
`loan-planner` aligns its buffers to 8 bytes so the page can read them as a `Float64Array`.

Three rules prevent most bugs:

- **Read `memory.buffer` again after every call.** A call that grows memory replaces the buffer,
  and an old view then reads zeros.
- **Reuse a large buffer.** An image of 1600 × 1066 pixels is 6.8 MB. `mandelbrot` and `image-lab`
  allocate it once and keep it until the size changes.
- **Return structured results as JSON text.** Write the JSON into memory in Rust, return its address
  and length, and call `JSON.parse` in the page. `text-tools` returns its word list this way.

## Talking to other packages

A page can send and receive on channels to other packages, with the user's permission. Load the
host's small library, declare the channels in `app.json`, and state `"format": 2`:

```html
<script src="/sdk/wardian.js"></script>
<script>
  wardian.channel('focus.session').send({ label: 'Writing', minutes: 25 })
    .catch(e => showHint('Allow the channel, or open this app inside Wardian.'));
</script>
```

[Apps that talk to each other](/docs/channels) explains channels in full.

## Standard parts

`/sdk/wardian.js` also defines `<wardian-progress>`, the standard progress bar. Use it for any work
longer than a second. Copy the rest of the component library into the package with
`wardian add button field card arrange apps/<name>`; [Components and Arrange](/docs/components)
lists them.

A page offers **Arrange** when it marks its parts with `data-panel` and loads `ui/arrange.js`.
`wardian check` warns about a page with no `data-panel`.

## Save as web page

When the viewer presses **Save as web page**, a small script that Wardian adds to the page copies
what the page shows. To put more in the file than fits on screen, set a function:

```js
window.wardianSnapshot = () => `<h2>Every row</h2>${tableHtml(allRows)}`;
```

It returns HTML text or an element, or a promise of either. Wardian cleans the answer by an
allow-list before it writes the file ([Package format §7.4](/docs/spec#7-distribution)).

## Without a page

A module with no page needs no HTML at all. Wardian draws one card per exported function, with one
input per parameter. Parameters and results must be numbers: `i32`, `i64`, `u32`, `u64`, `f32` or
`f64`. An `i64` arrives in JavaScript as a BigInt, so large whole numbers stay exact.
[`unit-converter`](/docs/examples#unit-converter) and [`number-lab`](/docs/examples#number-lab)
show this.

The host provides one import, `env.log(value)`, which writes to the host's output log. Every
other imported function becomes a stub that logs the call and returns 0.

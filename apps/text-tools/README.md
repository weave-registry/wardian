# Text tools

A Wardian **page app**. You type, paste or open a text file, and WebAssembly counts it, ranks its
words, hashes it with SHA-256 and changes its case, on every key press. Nothing leaves the browser.

Why a page: the module works on text and returns JSON, and WebAssembly functions pass only
numbers. So the page moves the text through the module's memory and draws the answers. It has one
job, the text in front of you, so it is one page and not a suite.

| File | What it is |
|---|---|
| `index.html` | The page: the text box and four panels (counts, top words, change case, SHA-256). |
| `app.js` | Copies the text into the module, calls each function, reads the answers back, draws them. Opens files. |
| `src/lib.rs` | `stats`, `top_words`, `sha256`, `change_case`, plus `alloc`, `dealloc` and `out_ptr`. SHA-256 is written out in full, with no crates. Unit tests at the end. |
| `app.wasm` | Built from `src/lib.rs` by `./build.sh`. |
| `ui/` | The Wardian component library: theme, button, field, card, badge, table, tabs, toast, switch and Arrange. |

| Panel | Shows |
|---|---|
| Text | The text box, Open file…, Sample text, Clear. You can also drop a file on the box. |
| Counts | Words, characters (with and without spaces), sentences, paragraphs, lines, unique words, average word length, reading and speaking time, size in UTF-8, the longest word. |
| Top words | The 10 to 100 most frequent words, with or without common English words. |
| Change case | UPPER, lower, Title Case, Sentence case and url-slug, with Copy and Download. |
| SHA-256 | The hash of the text, or of the open file's bytes. |

How data moves, once per change:

1. `app.js` encodes the text as UTF-8 and asks the module for space with `alloc(len)`.
2. It copies the bytes in once, then calls `stats`, `top_words`, `change_case` and `sha256` on
   the same address.
3. Each call puts its answer in one buffer the module owns and returns the length.
   `app.js` reads that many bytes at `out_ptr()`. It reads `memory.buffer` again after every call,
   because a call can grow memory.
4. `app.js` frees the space with `dealloc`.

Short text updates on every key. Text over 200,000 characters waits until typing pauses for a
quarter second. A 2 MB paste takes about 120 ms.

The hash of a file: a text box turns `\r\n` line ends into `\n`. So while the text is exactly what
the file held, the page hashes the file's own bytes, and the result matches
`shasum -a 256 <file>`. Once you edit the text, the badge says "of the text" and the page hashes
the text as UTF-8.

Arrange: each panel has `data-panel`, inside a `data-arrange-column`. Each viewer may reorder the
panels, move them between the columns, or hide them. A hidden panel still updates.

Limits:

- A word is a run of characters between spaces or dashes (— and –) with a letter or digit in it.
  "well-known" is one word.
- A sentence ends at `.`, `!`, `?` or `…` before a space. So "e.g. this" counts as two.
- Characters are Unicode code points. An emoji built from several code points, like a flag,
  counts as more than one.
- Reading time assumes 238 words a minute, speaking time 140.
- "Skip common words" knows about 80 English words only.
- url-slug turns common accented Latin letters into plain ones (é → e, ß → ss) and drops other
  scripts.
- Files up to 25 MB. A file that is not UTF-8 shows � where it cannot be read.

Change it: edit `src/lib.rs` and `app.js`, run `cargo test`, run `./build.sh`, then `wardian check .`.

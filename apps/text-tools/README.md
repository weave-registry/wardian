# Text tools

For anyone checking a piece of writing: type, paste or open a text file, and see at once how long
it is, how long it takes to read, which words it leans on, its SHA-256 hash, and the same text in
another case. WebAssembly does the work on every key press; nothing leaves the browser.

## What you put in

| Input | Type | Unit | Allowed | Default | Required |
|---|---|---|---|---|---|
| Text | long text | characters | any length; a file up to 25 MB | a sample passage, labelled | optional (empty shows empty states) |
| Open file… | a file | | a text file (`.txt`, `.md`, `.csv`, `.json`, `.log` and other `text/*`), up to 25 MB, UTF-8 | | optional |
| Show | one of 10, 25, 50, 100 | words | | 10 | required |
| Skip common words | on or off | | | on | |
| Case | one of UPPER, lower, Title Case, Sentence case, url-slug | | | UPPER | |

## What it shows

- **Counts:** words, characters with and without spaces, sentences, paragraphs, lines, unique
  words, average word length (letters), reading time (238 words a minute), speaking time (140 a
  minute), size in UTF-8 bytes, and the longest word.
- **Top words:** the most frequent words, how many times each appears, and a bar relative to the
  top word, with or without about 80 common English words.
- **Change case:** the text in the chosen case, to copy or download as a `.txt` file.
- **SHA-256:** the hash of the text as UTF-8 with `\n` line ends, or, while the text is exactly what
  an opened file held, of the file's own bytes, so it matches `shasum -a 256 <file>`.

## How it behaves

- **First screen:** a sample passage (the opening of Moby-Dick), marked **Sample text**, with every
  panel filled in.
- **Empty:** after Clear, each panel says what to do: type or paste text, or open a file.
- **As you type:** short text updates on every key; over 200,000 characters it waits until typing
  pauses for a quarter second. A 2 MB paste takes about 120 ms.
- **Errors:** a file over 25 MB is refused, and a file that is not UTF-8 opens with a warning; both
  are said under the text box (and in a toast), with what is allowed.
- **Success:** opening a file, copying and downloading each confirm with a toast.
- **Kept between visits:** nothing; the page is sealed.

## Layout

Two columns that Arrange may swap or join; one column under 820px. Reading order and tab order:

1. **Text** (side): the source badge (Sample text, Typed text, or the file's name), the text box
   (`textarea`, `size-full`, 14 rows, resizable), its hint, then **Open file…** (`input type="file"`
   with `accept`, behind the button), **Sample text**, **Clear**. File errors appear in the hint under
   the box, marked `aria-invalid` on the box while they apply.
2. **Counts** (main): a grid of figures, the timing note; empty state "Type, paste or open a text to
   count it."
3. **Top words** (main): the radio group **Show** (10, 25, 50, 100 words), the switch **Skip common
   words**, then the table (`num` on the rank and count).
4. **Change case** (main): the case tabs, the result, **Copy** and **Download .txt**.
5. **SHA-256** (main): what it is a hash of, the hash, **Copy**.

## Files

| File | What it is |
|---|---|
| `index.html` | The page: the text box and four panels. |
| `app.js` | One parser per input (the file, the word limit), the copying of text into the module, and the drawing. |
| `src/lib.rs` | `stats`, `top_words`, `sha256`, `change_case`, plus `alloc`, `dealloc` and `out_ptr`. SHA-256 is written out in full, with no crates. Unit tests at the end. |
| `app.wasm` | Built from `src/lib.rs` by `./build.sh`. |
| `ui/` | The Wardian component library. |

How data moves, once per change: `app.js` encodes the text as UTF-8, asks the module for space with
`alloc(len)`, copies the bytes in once, calls `stats`, `top_words`, `change_case` and `sha256` on the
same address, reads each answer at `out_ptr()` (reading `memory.buffer` again after every call,
because a call can grow memory), and frees the space with `dealloc`.

Limits:

- A word is a run of characters between spaces or dashes (— and –) with a letter or digit in it.
  "well-known" is one word.
- A sentence ends at `.`, `!`, `?` or `…` before a space. So "e.g. this" counts as two.
- Characters are Unicode code points. An emoji built from several code points, like a flag,
  counts as more than one.
- url-slug turns common accented Latin letters into plain ones (é → e, ß → ss) and drops other
  scripts.

Change it: edit `src/lib.rs` and `app.js`, run `cargo test`, run `./build.sh`, then `wardian check .`.

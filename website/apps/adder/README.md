# Adder

The smallest Wardian app: a **module** with no page. `app.wasm` exports a function `add`, and Wardian
draws the whole interface from it: a card with one input per parameter and a Run button.

| File | What it is |
|---|---|
| `app.wasm` | the WebAssembly module (41 bytes): one function, `add` |

There is no `app.json`, so the title is the folder name, and no page, so Wardian shows the module's
functions. Start here to see what Wardian does with no help at all; then read `unit-converter` and
`number-lab` for module apps with many functions, and their Rust source.

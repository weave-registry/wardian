// Loads app.wasm from next to this file and passes text through its memory.
// WebAssembly functions take only numbers, so text goes in as bytes at an address (see src/lib.rs).
const { instance } = await WebAssembly.instantiateStreaming(fetch(new URL('./app.wasm', import.meta.url)));
const wasm = instance.exports;

// Copies text into the module, calls fn(ptr, len), and returns [fn's result, the bytes read back as text].
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

const input = document.getElementById('text');
const out = document.getElementById('out');
function update() {
  const [count] = withText(input.value, wasm.words);
  const [, loud] = withText(input.value, wasm.upper);
  out.textContent = `${count} word${count === 1 ? '' : 's'}\n${loud}`;
}
input.addEventListener('input', update);
update();

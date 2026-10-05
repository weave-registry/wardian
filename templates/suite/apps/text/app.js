/* text: no view. Loads text.wasm through the kernel and provides analyze() to other apps.
   Provides: analyze.  Capabilities: asset (the frame itself has no network). */
Kernel.register({
  name: 'text',
  provides: ['analyze'],
  caps: ['asset'],
  init(ctx) {
    // Start loading now; calls that arrive first simply wait for it.
    const ready = ctx.asset('text.wasm')
      .then(bytes => WebAssembly.instantiate(bytes))
      .then(({ instance }) => instance.exports);

    // Copies text into the module's memory, calls fn(ptr, len), returns [result, text read back].
    function withText(wasm, text, fn) {
      const bytes = new TextEncoder().encode(text);
      const ptr = wasm.alloc(bytes.length);
      try {
        new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
        const result = fn(ptr, bytes.length);
        return [result, new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, ptr, bytes.length))];
      } finally {
        wasm.dealloc(ptr, bytes.length);
      }
    }

    ctx.provide({
      async analyze({ text }) {
        const wasm = await ready;
        const [words] = withText(wasm, String(text), wasm.words);
        const [, loud] = withText(wasm, String(text), wasm.upper);
        return { words, loud };
      }
    });
  }
});

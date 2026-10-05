/* engine: no view. Runs the amortization in WebAssembly (engine.wasm, loaded through the kernel).
   Provides: schedule.  Capabilities: asset. */
Kernel.register({
  name: 'engine',
  provides: ['schedule'],
  caps: ['asset'],
  init(ctx) {
    const ready = ctx.asset('engine.wasm')
      .then(bytes => WebAssembly.instantiate(bytes))
      .then(({ instance }) => instance.exports);

    // One run of the module: returns the monthly payment and rows of [interest, principal, balance].
    function run(wasm, { amount, rate, years, extra }) {
      const max = years * 12;
      const bytes = max * 3 * 8;
      const ptr = wasm.alloc(bytes);
      try {
        const months = wasm.schedule(amount, rate, years, extra, ptr, max);
        // Read memory.buffer after the call: memory may have grown.
        const flat = new Float64Array(wasm.memory.buffer, ptr, months * 3);
        const rows = [];
        for (let i = 0; i < months; i++) rows.push([flat[i * 3], flat[i * 3 + 1], flat[i * 3 + 2]]);
        return { payment: wasm.payment(amount, rate, years), rows };
      } finally {
        wasm.dealloc(ptr, bytes);
      }
    }

    ctx.provide({
      async schedule(loan) {
        return run(await ready, loan);
      }
    });
  }
});

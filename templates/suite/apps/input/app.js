/* input: the left column. Publishes what you type and remembers it.
   Emits: text:changed (retained, so an app that starts later still gets it).  Capabilities: storage. */
Kernel.register({
  name: 'input',
  emits: { 'text:changed': { retain: true } },
  caps: ['storage'],
  init(ctx) {
    const box = ctx.$('#text');
    box.value = ctx.store.get('text') ?? 'hello from a Wardian suite';

    let timer = null;
    function publish() {
      ctx.emit('text:changed', { text: box.value });
      ctx.store.set('text', box.value);
    }
    box.addEventListener('input', () => { clearTimeout(timer); timer = setTimeout(publish, 200); });
    publish();
  }
});

/* output: the main column. Asks the text app to analyse each new text, then shows the result.
   Listens: text:changed.  Calls: text.analyze. */
Kernel.register({
  name: 'output',
  listens: ['text:changed'],
  needs: ['text.analyze'],
  init(ctx) {
    let latest = 0;
    ctx.on('text:changed', async ({ text }) => {
      const mine = ++latest;
      try {
        const { words, loud } = await ctx.call('text', 'analyze', { text });
        if (mine !== latest) return;          // a newer text arrived while this one was working
        ctx.$('#count').textContent = `${words} word${words === 1 ? '' : 's'}`;
        ctx.$('#loud').textContent = loud;
      } catch (e) {
        ctx.$('#count').textContent = 'Error';
        ctx.$('#loud').textContent = e.message;
      }
    });
  }
});

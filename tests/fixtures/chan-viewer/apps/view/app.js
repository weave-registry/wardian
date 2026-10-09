Kernel.register({
  name: 'view',
  channels: { send: ['budget'], receive: ['budget'] },
  init(ctx) {
    ctx.channel('budget').on((data, { from, id, name }) => {
      ctx.$('#v').textContent = data.monthly;
      ctx.$('#from').textContent = 'from ' + from;
      ctx.$('#id').textContent = id;
      ctx.$('#name').textContent = String(name);
    }).catch(e => { ctx.$('#v').textContent = 'error: ' + e.message; });
    // The test sends from here, to prove a package does not hear its own message (SPEC 6.9.4).
    window.sendBudget = (data, opts) => ctx.channel('budget').send(data, opts).then(() => 'sent', e => 'error: ' + e.message);
  }
});

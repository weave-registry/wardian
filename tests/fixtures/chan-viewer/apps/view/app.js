Kernel.register({
  name: 'view',
  channels: { send: ['budget'], receive: ['budget'] },
  init(ctx) {
    ctx.channel('budget').on((data, { from }) => {
      ctx.$('#v').textContent = data.monthly;
      ctx.$('#from').textContent = 'from ' + from;
    }).catch(e => { ctx.$('#v').textContent = 'error: ' + e.message; });
    // The test sends from here, to prove a package does not hear its own message (SPEC 6.9.4).
    window.sendBudget = data => ctx.channel('budget').send(data).then(() => 'sent', e => 'error: ' + e.message);
  }
});

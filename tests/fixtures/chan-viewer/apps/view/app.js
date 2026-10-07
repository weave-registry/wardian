Kernel.register({
  name: 'view',
  channels: { receive: ['budget'] },
  init(ctx) {
    ctx.channel('budget').on((data, { from }) => {
      ctx.$('#v').textContent = data.monthly;
      ctx.$('#from').textContent = 'from ' + from;
    }).catch(e => { ctx.$('#v').textContent = 'error: ' + e.message; });
  }
});

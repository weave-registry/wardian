/* board: the hosts and their errors. */
Kernel.register({
  name: 'board',
  init(ctx) {
    for (const [host, errors] of [['web-1', 40], ['web-2', 3], ['db-1', 12]]) {
      const tr = ctx.el('tr');
      for (const v of [host, errors]) { const td = ctx.el('td'); td.textContent = String(v); tr.append(td); }
      ctx.$('tbody').append(tr);
    }
  },
});

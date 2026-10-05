/* planner: no view. Turns each change of the inputs into one plan that every viewer shares, so the
   engine runs once per change instead of once per viewer.
   Listens: loan:changed.  Calls: engine.schedule.  Emits: plan:ready (retained). */
Kernel.register({
  name: 'planner',
  listens: ['loan:changed'],
  needs: ['engine.schedule'],
  emits: { 'plan:ready': { retain: true } },
  init(ctx) {
    let latest = 0;
    const sum = (rows, col) => rows.reduce((t, r) => t + r[col], 0);

    ctx.on('loan:changed', async loan => {
      const mine = ++latest;
      if (!loan.valid) { ctx.emit('plan:ready', { ok: false, loan }); return; }
      try {
        // The plan as asked, and the same loan without the extra payment, to show what it saves.
        const [withExtra, base] = await Promise.all([
          ctx.call('engine', 'schedule', loan),
          ctx.call('engine', 'schedule', { ...loan, extra: 0 }),
        ]);
        if (mine !== latest) return;    // newer inputs arrived while the engine was working
        const interest = sum(withExtra.rows, 0), baseInterest = sum(base.rows, 0);
        ctx.emit('plan:ready', {
          ok: true,
          loan,
          payment: withExtra.payment,
          months: withExtra.rows.length,
          interest,
          total: loan.amount + interest,
          rows: withExtra.rows,
          baseline: { months: base.rows.length, interest: baseInterest, balances: base.rows.map(r => r[2]) },
          saved: { months: base.rows.length - withExtra.rows.length, interest: baseInterest - interest },
        });
      } catch (e) {
        if (mine === latest) ctx.emit('plan:ready', { ok: false, loan, error: e.message });
      }
    });
  }
});

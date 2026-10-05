/* summary: the headline numbers of the plan.  Listens: plan:ready. */
Kernel.register({
  name: 'summary',
  listens: ['plan:ready'],
  init(ctx) {
    const tile = (label, value, note) => {
      const t = ctx.el('div'); t.className = 'tile';
      const l = ctx.el('div'); l.className = 'label'; l.textContent = label;
      const v = ctx.el('div'); v.className = 'value'; v.textContent = value;
      t.append(l, v);
      if (note) { const n = ctx.el('div'); n.className = 'label'; n.textContent = note; t.append(n); }
      return t;
    };
    ctx.on('plan:ready', plan => {
      const tiles = ctx.$('#tiles'), saved = ctx.$('#saved');
      if (!plan.ok) {
        tiles.replaceChildren(); saved.textContent = plan.error || plan.loan.problem || 'Check the loan details.';
        ctx.$('#sub').textContent = '';
        return;
      }
      const { loan } = plan, cur = loan.currency, money = (v, d) => Fmt.money(v, cur, d);
      ctx.$('#sub').textContent = `${money(loan.amount)} at ${loan.rate}% over ${loan.years} years`;
      tiles.replaceChildren(
        tile('Monthly payment', money(plan.payment + loan.extra, 2), loan.extra ? `${money(plan.payment, 2)} + ${money(loan.extra, 2)} extra` : ''),
        tile('Total interest', money(plan.interest)),
        tile('Total paid', money(plan.total)),
        tile('Paid off', Fmt.month(loan.start, plan.months - 1), Fmt.span(plan.months)),
      );
      saved.textContent = loan.extra > 0 && plan.saved.months > 0
        ? `Paying ${money(loan.extra)} extra a month saves ${money(plan.saved.interest)} in interest and finishes ${Fmt.span(plan.saved.months)} sooner.`
        : 'Add an extra monthly payment to see how much interest it saves.';
    });
  }
});

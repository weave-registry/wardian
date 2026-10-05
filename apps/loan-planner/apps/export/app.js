/* export: saves the schedule as CSV through the host's download capability.
   Listens: plan:ready.  Capabilities: claude:downloads. */
Kernel.register({
  name: 'export',
  listens: ['plan:ready'],
  caps: ['claude:downloads'],
  init(ctx) {
    let plan = null, downloads = null;
    const button = ctx.$('#csv'), note = ctx.$('#note');
    const sync = () => {
      button.disabled = !(plan && plan.ok && downloads);
      ctx.$('#sub').textContent = plan && plan.ok ? `${plan.months} rows` : '';
      if (!downloads) note.textContent = 'Downloads are not available in this host.';
    };
    ctx.cap('downloads').then(d => { downloads = d; sync(); });
    ctx.on('plan:ready', p => { plan = p; note.textContent = ''; sync(); });

    const cents = v => v.toFixed(2);
    button.addEventListener('click', async () => {
      const { loan, rows } = plan;
      const lines = ['payment,month,payment_amount,interest,principal,balance'];
      rows.forEach(([interest, principal, balance], i) => {
        lines.push([i + 1, Fmt.month(loan.start, i).replace(',', ''), cents(interest + principal), cents(interest), cents(principal), cents(balance)].join(','));
      });
      const name = `loan-${loan.amount}-${loan.rate}pct-${loan.years}y.csv`.replace(/[^\w.-]/g, '_');
      try {
        await downloads.save({ filename: name, data: lines.join('\n') + '\n' });
        note.textContent = `Saved ${name}.`;
      } catch (e) {
        note.textContent = 'Could not save: ' + e.message;
      }
    });
  }
});

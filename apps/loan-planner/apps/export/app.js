/* export: saves the schedule as CSV through the host's download capability.
   Listens: plan:ready.  Capabilities: claude:downloads. */
Kernel.register({
  name: 'export',
  listens: ['plan:ready'],
  caps: ['claude:downloads'],
  init(ctx) {
    let plan = null, downloads = null;
    const button = ctx.$('#csv'), note = ctx.$('#note');
    let busy = false, said = '';
    // The button is disabled only with its reason beside it.
    const sync = () => {
      button.disabled = busy || !(plan && plan.ok && downloads);
      button.textContent = busy ? 'Saving…' : 'Download schedule (CSV)';
      ctx.$('#sub').textContent = plan && plan.ok ? `${plan.months} rows` : '';
      note.textContent = said || (!downloads ? 'Downloads are not available in this Wardian.'
        : !(plan && plan.ok) ? 'Fix the loan details to download the schedule.' : '');
    };
    ctx.cap('downloads').then(d => { downloads = d; sync(); }, () => sync());
    ctx.on('plan:ready', p => { plan = p; said = ''; sync(); });

    const cents = v => v.toFixed(2);
    button.addEventListener('click', async () => {
      const { loan, rows } = plan;
      const lines = ['payment,month,payment_amount,interest,principal,balance'];
      rows.forEach(([interest, principal, balance], i) => {
        lines.push([i + 1, Fmt.month(loan.start, i).replace(',', ''), cents(interest + principal), cents(interest), cents(principal), cents(balance)].join(','));
      });
      const name = `loan-${loan.amount}-${loan.rate}pct-${loan.years}y.csv`.replace(/[^\w.-]/g, '_');
      busy = true; said = ''; sync();
      try {
        await downloads.save({ filename: name, data: lines.join('\n') + '\n' });
        said = `Saved ${name}.`;
      } catch (e) {
        said = 'Could not save it: ' + e.message + '. Try again.';
      }
      busy = false; sync();
    });
  }
});

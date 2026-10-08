/* inputs: the left column. Reads the loan, checks it, remembers it, and publishes it.
   Emits: loan:changed (retained: it is the current state).  Capabilities: storage. */
Kernel.register({
  name: 'inputs',
  emits: { 'loan:changed': { retain: true } },
  caps: ['storage'],
  init(ctx) {
    const next = new Date(new Date().getFullYear(), new Date().getMonth() + 1, 1);   // first of next month
    const DEFAULTS = {
      amount: 300000, rate: 6, years: 30, extra: 0, currency: 'USD',
      start: `${next.getFullYear()}-${String(next.getMonth() + 1).padStart(2, '0')}`,
    };
    const fields = ['amount', 'rate', 'years', 'extra', 'start', 'currency'];
    const saved = ctx.store.get('loan') || {};
    for (const f of fields) ctx.$('#' + f).value = saved[f] ?? DEFAULTS[f];

    function read() {
      const n = id => Number(ctx.$('#' + id).value);
      const loan = { amount: n('amount'), rate: n('rate'), years: Math.round(n('years')), extra: n('extra') || 0,
                     start: ctx.$('#start').value || DEFAULTS.start, currency: ctx.$('#currency').value };
      let problem = '';
      if (!(loan.amount > 0)) problem = 'Enter an amount above zero.';
      else if (!(loan.rate >= 0 && loan.rate <= 50)) problem = 'Enter a rate from 0 to 50%.';
      else if (!(loan.years >= 1 && loan.years <= 50)) problem = 'Enter a term from 1 to 50 years.';
      else if (!(loan.extra >= 0)) problem = 'The extra payment cannot be negative.';
      return { ...loan, valid: !problem, problem };
    }

    let timer = null;
    function publish() {
      const loan = read();
      ctx.$('#problem').textContent = loan.problem;
      ctx.emit('loan:changed', loan);
      if (loan.valid) ctx.store.set('loan', Object.fromEntries(fields.map(f => [f, loan[f]])));
    }
    ctx.$('#form').addEventListener('input', () => { clearTimeout(timer); timer = setTimeout(publish, 150); });
    ctx.$('#form').addEventListener('submit', e => e.preventDefault());
    publish();
  }
});

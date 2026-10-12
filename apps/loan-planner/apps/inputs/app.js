/* inputs: the left column. Reads the loan with Loan's parsers (shared/loan.js), checks each field
   when the user leaves it, remembers a valid loan and publishes it.
   Emits: loan:changed (retained: it is the current state).  Capabilities: storage. */
Kernel.register({
  name: 'inputs',
  emits: { 'loan:changed': { retain: true } },
  caps: ['storage'],
  init(ctx) {
    const names = Object.keys(Loan.fields);
    const sample = Loan.sample();
    const saved = ctx.store.get('loan');
    for (const f of names) ctx.$('#' + f).value = (saved || {})[f] ?? sample[f];
    ctx.$('#sample').hidden = !!saved;

    // Each field's own hint, kept so an error can be put back to it.
    const hints = Object.fromEntries(names.map(f => [f, ctx.$('#' + f + '-hint').textContent]));
    const values = () => Object.fromEntries(names.map(f => [f, ctx.$('#' + f).value]));
    function mark(f, error) {
      const input = ctx.$('#' + f), hint = ctx.$('#' + f + '-hint');
      if (error) { input.setAttribute('aria-invalid', 'true'); hint.textContent = error; hint.classList.add('error'); }
      else { input.removeAttribute('aria-invalid'); hint.textContent = hints[f]; hint.classList.remove('error'); }
    }
    const units = () => { for (const s of ctx.$$('.cur')) s.textContent = ctx.$('#currency').value; };

    function publish() {
      const { loan, errors, valid } = Loan.read(values());
      const count = Object.keys(errors).length;
      ctx.$('#problem').textContent = valid ? '' : count === 1 ? 'Fix the highlighted field to see results.' : `Fix the ${count} highlighted fields to see results.`;
      ctx.emit('loan:changed', valid ? { ...loan, valid: true, problem: '' } : { ...loan, valid: false, problem: ctx.$('#problem').textContent });
      if (valid) ctx.store.set('loan', loan);
    }

    let timer = null;
    const form = ctx.$('#form');
    // While typing: clear a field's error as soon as it is valid, and recalculate; never mark a
    // half-typed value wrong.
    form.addEventListener('input', e => {
      const f = e.target.name;
      if (f && e.target.getAttribute('aria-invalid') === 'true' && !('error' in Loan.fields[f](e.target.value))) mark(f, '');
      if (f === 'currency') units();
      ctx.$('#sample').hidden = true;
      clearTimeout(timer); timer = setTimeout(publish, 150);
    });
    // When the user leaves a field: check it, and say what is allowed.
    form.addEventListener('change', e => {
      const f = e.target.name;
      if (f) { const r = Loan.fields[f](e.target.value); mark(f, r.error || ''); }
    });
    // Enter submits: check everything, show every error, focus the first one.
    form.addEventListener('submit', e => {
      e.preventDefault();
      const { errors } = Loan.read(values());
      for (const f of names) mark(f, errors[f] || '');
      const first = names.find(f => errors[f]);
      if (first) ctx.$('#' + first).focus();
      publish();
    });
    units();
    publish();
  }
});

/* Loan: the loan's fields, one parser per input (pure: text in, a typed value or an error out). The
   inputs part uses them as the user types; anything else that reads a loan (an import, a test) uses
   the same rules. Each rule is the README's field spec. */
const Loan = Object.freeze({
  /** Each field: its parser. A parser returns { value } or { error } saying what is allowed. */
  fields: {
    amount(text) {
      const v = Number(String(text).trim());
      if (String(text).trim() === '') return { error: 'Enter the amount you borrow.' };
      if (!Number.isFinite(v) || v < 1 || v > 100000000) return { error: 'Enter an amount from 1 to 100,000,000.' };
      if (!Number.isInteger(v)) return { error: 'Enter a whole amount, without cents.' };
      return { value: v };
    },
    rate(text) {
      const v = Number(String(text).trim());
      if (String(text).trim() === '') return { error: 'Enter the interest rate.' };
      if (!Number.isFinite(v) || v < 0 || v > 50) return { error: 'Enter a rate from 0 to 50%.' };
      return { value: v };
    },
    years(text) {
      const v = Number(String(text).trim());
      if (String(text).trim() === '') return { error: 'Enter the term in years.' };
      if (!Number.isInteger(v) || v < 1 || v > 50) return { error: 'Enter a whole number of years from 1 to 50.' };
      return { value: v };
    },
    extra(text) {
      const t = String(text).trim();
      if (t === '') return { value: 0 };
      const v = Number(t);
      if (!Number.isFinite(v) || v < 0 || v > 10000000) return { error: 'Enter an extra payment from 0 to 10,000,000, or leave it empty.' };
      return { value: Math.round(v * 100) / 100 };
    },
    start(text) {
      const t = String(text).trim();
      if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(t)) return { error: 'Choose the month of the first payment.' };
      return { value: t };
    },
    currency(text) {
      const t = String(text).trim().toUpperCase();
      return Loan.currencies.includes(t) ? { value: t } : { error: 'Choose a currency from the list.' };
    },
  },
  currencies: ['USD', 'EUR', 'GBP', 'CAD', 'AUD', 'CHF', 'JPY', 'INR'],
  /** The sample loan the app opens with, the first payment next month. */
  sample(now = new Date()) {
    const next = new Date(now.getFullYear(), now.getMonth() + 1, 1);
    return { amount: 300000, rate: 6, years: 30, extra: 0, currency: 'USD',
      start: `${next.getFullYear()}-${String(next.getMonth() + 1).padStart(2, '0')}` };
  },
  /** Reads every field: { loan, errors } with a typed loan only when every field is valid. */
  read(values) {
    const loan = {}, errors = {};
    for (const [name, parse] of Object.entries(Loan.fields)) {
      const r = parse(values[name] ?? '');
      if ('error' in r) errors[name] = r.error; else loan[name] = r.value;
    }
    return { loan, errors, valid: Object.keys(errors).length === 0 };
  },
});

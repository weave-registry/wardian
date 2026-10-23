/* ai: the lab's AI, out of sight (ADR-2610131300). Its skill (skill.md) and tools (tools.json) are data
   that Wardian's App settings show, edit and improve; this code holds only each tool's run, on the fit,
   the checks and the description it hears. The numbers are the page's: Claude only reads them.
   Listens: analysis:ready, checks:ready, context:changed.
   Provides: status ({ claude }), diagnose (→ { text } or { error }), whatIf ({ text } → factors or
   { error }), cancel, chat (Wardian's Ask).  Capabilities: claude:sample, asset. */
Kernel.register({
  name: 'ai',
  listens: ['analysis:ready', 'checks:ready', 'context:changed'],
  provides: ['status', 'diagnose', 'whatIf', 'cancel', 'chat'],
  caps: ['claude:sample', 'asset'],
  init(ctx){
    const {fmt, fmtRange, clamp, SIG} = Lib;
    let A = null, checks = [], context = '', ctl = null;
    ctx.on('analysis:ready', a => { A = a; });
    ctx.on('checks:ready', c => { checks = c.items || []; });
    ctx.on('context:changed', c => { context = c.text || ''; });
    const fitted = () => !!(A && A.ok);

    function fit(){
      if (!fitted()) return {none: 'The lab has no fit yet: it needs at least 4 runs at 3 or more different loads.'};
      const f0 = A.fit, ci = A.ci, pk = f0.peak;
      const out = {units: {load: A.units.n, throughput: A.units.x},
        alpha: fmt(f0.alpha, SIG.alpha) + ' [' + fmtRange(ci.alpha[0], ci.alpha[1], SIG.alpha) + ']',
        beta: fmt(f0.beta, SIG.beta) + ' [' + fmtRange(ci.beta[0], ci.beta[1], SIG.beta) + ']',
        lambda: fmt(f0.lambda, SIG.lambda) + ' [' + fmtRange(ci.lambda[0], ci.lambda[1], SIG.lambda) + ']',
        r_squared: f0.r2.toFixed(3), typical_error: (f0.rmsePct*100).toFixed(1) + '%', highest_measured_load: fmt(A.rows[A.rows.length-1].n)};
      if (pk.kind === 'peak'){
        out.peak_load = fmt(pk.nStar, SIG.nStar) + ' [' + fmtRange(ci.nStar[0], ci.nStar[1], SIG.nStar) + ']';
        out.peak_throughput = fmt(pk.xMax, SIG.xMax) + ' [' + fmtRange(ci.xMax[0], ci.xMax[1], SIG.xMax) + ']';
      } else if (pk.kind === 'ceiling') out.peak = 'no peak (beta is about zero); throughput ceiling lambda/alpha = ' + fmt(pk.xMax, SIG.xMax);
      else out.peak = 'no peak and no ceiling (alpha and beta both about zero)';
      return out;
    }
    function rows(from = 1, max = 50){
      if (!A || !Array.isArray(A.rows)) return [];
      const flags = new Set(A.flags || []);
      return A.rows.slice(from - 1, from - 1 + max).map((r, i) => ({row: from + i, load: fmt(r.n), throughput: fmt(r.x, 4),
        ...(r.r ? {response_time: r.r} : {}), ...(flags.has(from - 1 + i) ? {off_curve: true} : {})}));
    }

    const ai = WardianUI.agentApp(ctx, { maxSteps: 6, run: {
      fit_results: async () => fit(),
      data_checks: async () => checks.map(c => ({level: c.level, title: c.title, detail: c.detail})),
      measurements: async ({from_row = 1, max_rows = 50}) => ({rows: rows(from_row, max_rows), total: A && A.rows ? A.rows.length : 0}),
      system_description: async () => ({description: context || '(none given)'}),
    } });
    const failed = (code, message) => ({error: {code, message}});

    ctx.provide({
      status: async () => ({claude: !!(await ctx.cap('sample').catch(() => null))}),
      /** The diagnosis, in Markdown: { text } or { error: { code, message } }. */
      async diagnose(){
        if (!fitted()) return failed('no_fit', 'Add measurements first.');
        if (ctl) ctl.abort();
        const mine = ctl = new AbortController();
        try {
          const task = 'Write the diagnosis of this USL fit, as the skill says. Everything you need is here:\n' +
            JSON.stringify({system_description: context || '(none given)', fit: fit(), data_checks: checks.map(c => ({level: c.level, title: c.title, detail: c.detail})), measurements: rows()}) +
            '\nAnswer with the Markdown as a string.';
          const out = await ai.ask(task, {signal: mine.signal});
          if (out.error) return {error: out.error};
          return typeof out.answer === 'string' && out.answer.trim() ? {text: out.answer} : failed('bad_reply', 'Claude did not write a diagnosis.');
        } finally { if (ctl === mine) ctl = null; }
      },
      /** A proposed change as slider factors: { a, b, l, rationale } or { error }. The model never does the arithmetic. */
      async whatIf({text}){
        if (!fitted()) return failed('no_fit', 'Add measurements first.');
        const f0 = A.fit;
        const task = 'Turn this proposed change into factors, as the skill says.\nThe model: alpha = ' + fmt(f0.alpha, SIG.alpha) + ', beta = ' + fmt(f0.beta, SIG.beta) +
          ', lambda = ' + fmt(f0.lambda, SIG.lambda) + ' ' + A.units.x + '; load is in ' + A.units.n + '.\nSystem description: ' + (context || '(none given)') +
          '\nThe change: "' + String(text).slice(0, 200) + '"';
        const out = await ai.ask(task);
        if (out.error) return {error: out.error};
        const r = out.answer && typeof out.answer === 'object' ? out.answer : {};
        const num = (v, lo, hi) => (typeof v === 'number' && isFinite(v)) ? clamp(Math.round(v*20)/20, lo, hi) : 1;
        return {a: num(r.alphaFactor, 0, 3), b: num(r.betaFactor, 0, 3), l: num(r.lambdaFactor, 0.25, 3), rationale: typeof r.rationale === 'string' ? r.rationale : ''};
      },
      cancel: () => { if (ctl) ctl.abort(); return true; },
      chat: ({messages}, call) => ai.chat(messages, call),
    });
  }
});

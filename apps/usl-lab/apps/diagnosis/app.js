/* diagnosis: asks Claude to interpret the computed results, and to translate plain-language what-ifs.
   Listens: analysis:ready, checks:ready, context:changed.  Provides: parseWhatIf.
   Emits: capability:ai, diagnosis:updated (both retained).  Capabilities: claude:sample. */
Kernel.register({
  name: 'diagnosis',
  emits: {'capability:ai': {retain: true}, 'diagnosis:updated': {retain: true}},
  listens: ['analysis:ready', 'checks:ready', 'context:changed'],
  provides: ['parseWhatIf'],
  caps: ['claude:sample'],
  init(ctx){
    const $ = ctx.$, {fmt, fmtRange, clamp} = Lib;
    let A = null, checks = [], context = '', sample = null, text = '', stale = false, busy = false, ctl = null;

    ctx.root.hidden = true;
    ctx.emit('diagnosis:updated', {text: '', stale: false});
    ctx.cap('sample').then(s => {
      sample = s;
      ctx.emit('capability:ai', {available: !!s});
      ctx.root.hidden = !s;
    });

    ctx.on('analysis:ready', a => {
      A = a;
      $('#btnAi').disabled = !(A.ok) || busy;
      if (text){ stale = true; $('#aiOut').classList.add('stale'); if (!$('#aiErr').textContent) $('#aiErr').textContent = 'The data changed after this diagnosis was written. Interpret again to refresh it.'; ctx.emit('diagnosis:updated', {text, stale}); }
    });
    ctx.on('checks:ready', c => { checks = c.items; });
    ctx.on('context:changed', c => { context = c.text; });

    function renderMd(host, md){
      host.replaceChildren();
      let list = null, ltype = null;
      const inline = (into, s) => s.split(/(\*\*[^*]+\*\*|`[^`]+`)/).forEach(part => {
        if (!part) return;
        if (part.startsWith('**') && part.endsWith('**') && part.length > 4){ const b = ctx.el('strong'); b.textContent = part.slice(2, -2); into.appendChild(b); }
        else if (part.startsWith('`') && part.endsWith('`') && part.length > 2){ const c = ctx.el('code'); c.textContent = part.slice(1, -1); into.appendChild(c); }
        else into.appendChild(ctx.text(part));
      });
      md.split('\n').forEach(raw => {
        const line = raw.trim(); let mt;
        if (!line){ list = null; return; }
        if ((mt = line.match(/^#{1,4}\s+(.*)$/))){ list = null; const h = ctx.el('h3'); inline(h, mt[1]); host.appendChild(h); }
        else if ((mt = line.match(/^[-*]\s+(.*)$/))){ if (ltype !== 'ul' || !list){ list = ctx.el('ul'); ltype = 'ul'; host.appendChild(list); } const li = ctx.el('li'); inline(li, mt[1]); list.appendChild(li); }
        else if ((mt = line.match(/^\d+[.)]\s+(.*)$/))){ if (ltype !== 'ol' || !list){ list = ctx.el('ol'); ltype = 'ol'; host.appendChild(list); } const li = ctx.el('li'); inline(li, mt[1]); list.appendChild(li); }
        else { list = null; const p = ctx.el('p'); inline(p, line); host.appendChild(p); }
      });
    }

    const GONE = ['not_granted', 'sampling_disabled', 'not_declared', 'capability_disabled', 'capability_removed'];
    function errCopy(code){
      switch (code){
        case 'rate_limited': return 'You have hit a usage limit. Try again in a while.';
        case 'session_expired': return 'Your Claude session expired. Sign in again, then retry.';
        case 'refused': return 'Claude declined this request. Try changing the system description or the request.';
        case 'invalid_json': return 'Claude’s answer could not be read. Try again or rephrase.';
        case 'prompt_too_large': return 'The data is too large to send. Use fewer runs.';
        default: return 'Something went wrong reaching Claude. Try again.';
      }
    }
    function lose(){ sample = null; ctx.root.hidden = true; ctx.emit('capability:ai', {available: false}); }

    function contextBlock(){
      const f0 = A.fit, ci = A.ci, u = A.units, pk = f0.peak, flags = new Set(A.flags), L = [];
      L.push('System description from the user: ' + (context || '(none given)'));
      L.push('Units: load is in ' + u.n + '; throughput is in ' + u.x + '.', '');
      L.push('USL fit (computed by the page; 90% bootstrap ranges in brackets):');
      L.push('- alpha (contention) = ' + fmt(f0.alpha, 3) + ' [' + fmtRange(ci.alpha[0], ci.alpha[1], 3) + ']');
      L.push('- beta (coherency) = ' + fmt(f0.beta, 3) + ' [' + fmtRange(ci.beta[0], ci.beta[1], 3) + ']');
      L.push('- lambda (single-unit throughput) = ' + fmt(f0.lambda, 4) + ' [' + fmtRange(ci.lambda[0], ci.lambda[1], 4) + ']');
      if (pk.kind === 'peak'){
        L.push('- peak load N* = ' + fmt(pk.nStar) + ' [' + fmtRange(ci.nStar[0], ci.nStar[1]) + ']');
        L.push('- peak throughput = ' + fmt(pk.xMax, 4) + ' [' + fmtRange(ci.xMax[0], ci.xMax[1], 4) + ']');
      } else if (pk.kind === 'ceiling') L.push('- no peak (beta is about zero); throughput ceiling lambda/alpha = ' + fmt(pk.xMax, 4));
      else L.push('- no peak and no ceiling (alpha and beta both about zero)');
      L.push('- R squared = ' + f0.r2.toFixed(3) + '; typical error = ' + (f0.rmsePct*100).toFixed(1) + '%; highest measured load = ' + fmt(A.rows[A.rows.length-1].n), '');
      L.push('Data checks:');
      checks.forEach(c => L.push('- [' + c.level + '] ' + c.title + ': ' + c.detail));
      L.push('', 'Measurements (load, throughput' + (A.rows.some(r => r.r) ? ', response time' : '') + '):');
      A.rows.forEach((r, i) => L.push(fmt(r.n) + ', ' + fmt(r.x, 4) + (r.r ? ', ' + r.r : '') + (flags.has(i) ? '  (flagged off-curve)' : '')));
      return L.join('\n');
    }

    async function run(){
      if (!sample || !A || !A.ok || busy) return;
      const prompt = 'You are a performance engineer interpreting a Universal Scalability Law (USL) fit. Every number below was computed by the page\'s fitting code. Do not recompute or alter them, and do no arithmetic beyond simple comparisons.\n\n' +
        contextBlock() + '\n\n' +
        'Write the diagnosis in Markdown, under 350 words, using exactly these section headings:\n## What the curve says\n## Likely causes\n## Experiments to confirm\n## What to change first\n\n' +
        'Rules: tie each hypothesis to alpha or beta evidence and label it as a hypothesis. If the predicted peak is beyond the measured loads, say N* is an extrapolation. If checks flag problems, say how they limit confidence. Do not invent details about the system beyond the description; if no description was given, give generic causes and say so. Keep advice specific and ordered by expected payoff.';
      const out = $('#aiOut'), err = $('#aiErr');
      ctl = new AbortController(); busy = true; stale = false; err.textContent = '';
      $('#btnAi').disabled = true; $('#btnAiStop').hidden = false;
      out.className = 'ai-out'; out.innerHTML = '<p class="thinking">Thinking…</p>';
      try {
        const res = await sample(prompt, {signal: ctl.signal, onText: ({text: t}) => { text = t; renderMd(out, t); }});
        text = res.text; renderMd(out, text);
        if (res.truncated) err.textContent = 'The answer was cut short. Run it again for a complete version.';
      } catch (e) {
        if (e && e.text){ text = e.text; renderMd(out, text); } else { text = ''; out.innerHTML = ''; }
        if (!(e && e.code === 'cancelled')){ if (e && GONE.includes(e.code)) lose(); else err.textContent = errCopy(e && e.code); }
      } finally {
        busy = false; $('#btnAi').disabled = !(A && A.ok); $('#btnAiStop').hidden = true;
        $('#btnAi').textContent = text ? 'Interpret again' : 'Interpret results';
        ctx.emit('diagnosis:updated', {text, stale});
      }
    }
    $('#btnAi').addEventListener('click', run);
    $('#btnAiStop').addEventListener('click', () => ctl && ctl.abort());

    // Provided to the what-if app: plain language -> slider factors. The model never does the arithmetic.
    ctx.provide({
      async parseWhatIf({text: ask}){
        if (!sample) throw new Error('AI is not available in this view.');
        if (!A || !A.ok) throw new Error('Add measurements first.');
        const f0 = A.fit, u = A.units;
        const prompt = 'A Universal Scalability Law model of a system has alpha (serialized fraction) = ' + fmt(f0.alpha, 3) + ', beta (coherency cost) = ' + fmt(f0.beta, 3) + ', lambda (single-unit throughput) = ' + fmt(f0.lambda, 4) + ' ' + u.x + '. Load is measured in ' + u.n + '.\n' +
          'System description: ' + (context || '(none given)') + '\n\n' +
          'The user proposes this change: "' + String(ask).replace(/"/g, "'") + '"\n\n' +
          'Convert it into multiplicative factors: alphaFactor scales the serialized share of work (locks, single writers, shared queues), betaFactor scales the coordination cost between workers (crosstalk, cache sync, consensus), lambdaFactor scales single-worker throughput. 1 means unchanged, 0.5 halves it. Keep each factor between 0 and 3. If the change cannot be mapped to these, return all 1 and say why.\n' +
          'Reply with only JSON: {"alphaFactor":1,"betaFactor":1,"lambdaFactor":1,"rationale":"one or two sentences explaining the mapping and any assumption"}';
        try {
          const r = await sample.json(prompt, {modelTier: 'quick'});
          const num = (v, lo, hi) => (typeof v === 'number' && isFinite(v)) ? clamp(Math.round(v*20)/20, lo, hi) : 1;
          return {a: num(r.alphaFactor, 0, 3), b: num(r.betaFactor, 0, 3), l: num(r.lambdaFactor, 0.25, 3), rationale: typeof r.rationale === 'string' ? r.rationale : ''};
        } catch (e) {
          if (e && GONE.includes(e.code)) lose();
          throw new Error(errCopy(e && e.code));
        }
      }
    });
  }
});

/* diagnosis: the Diagnosis panel. Asks the out-of-sight ai part to interpret the computed results, and
   passes plain-language what-ifs to it for the what-if part.
   Listens: analysis:ready.  Provides: parseWhatIf.  Needs: ai.status, ai.diagnose, ai.whatIf, ai.cancel.
   Emits: capability:ai, diagnosis:updated (both retained). */
Kernel.register({
  name: 'diagnosis',
  emits: {'capability:ai': {retain: true}, 'diagnosis:updated': {retain: true}},
  listens: ['analysis:ready'],
  provides: ['parseWhatIf'],
  needs: ['ai.status', 'ai.diagnose', 'ai.whatIf', 'ai.cancel'],
  init(ctx){
    const $ = ctx.$;
    let A = null, available = false, text = '', stale = false, busy = false;

    ctx.root.hidden = true;
    ctx.emit('diagnosis:updated', {text: '', stale: false});
    ctx.call('ai', 'status', {}).catch(() => ({})).then(s => {
      available = !!s.claude;
      ctx.emit('capability:ai', {available});
      ctx.root.hidden = !available;
    });

    ctx.on('analysis:ready', a => {
      A = a;
      $('#btnAi').disabled = !(A.ok) || busy;
      if (text){ stale = true; $('#aiOut').classList.add('stale'); if (!$('#aiErr').textContent) $('#aiErr').textContent = 'The data changed after this diagnosis was written. Interpret again to refresh it.'; ctx.emit('diagnosis:updated', {text, stale}); }
    });

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

    const GONE = ['not_granted', 'unavailable'];
    function errCopy(code){
      switch (code){
        case 'rate_limited': return 'You have hit a usage limit. Try again in a while.';
        case 'over_budget': return 'This app has used its Claude tokens for today. An admin can raise its cap in Settings → Claude.';
        case 'refused': return 'Claude declined this request. Try changing the system description or the request.';
        case 'invalid_json': case 'bad_reply': case 'max_steps': return 'Claude’s answer could not be read. Try again or rephrase.';
        case 'prompt_too_large': return 'The data is too large to send. Use fewer runs.';
        case 'bad_repository': return 'The AI’s skill and tools could not be read. Check them in ⋯ → AI skill and tools.';
        case 'no_fit': return 'Add measurements first.';
        default: return 'Something went wrong reaching Claude. Try again.';
      }
    }
    function lose(){ available = false; ctx.root.hidden = true; ctx.emit('capability:ai', {available: false}); }

    async function run(){
      if (!available || !A || !A.ok || busy) return;
      const out = $('#aiOut'), err = $('#aiErr');
      busy = true; stale = false; err.textContent = '';
      $('#btnAi').disabled = true; $('#btnAiStop').hidden = false;
      out.className = 'ai-out'; out.innerHTML = '<p class="thinking">Thinking…</p>';
      try {
        const r = await ctx.call('ai', 'diagnose', {});
        if (r.error) throw r.error;
        text = r.text; renderMd(out, text);
      } catch (e) {
        text = ''; out.replaceChildren();
        if (!(e && e.code === 'cancelled')){ if (e && GONE.includes(e.code)) lose(); else err.textContent = errCopy(e && e.code); }
      } finally {
        busy = false; $('#btnAi').disabled = !(A && A.ok); $('#btnAiStop').hidden = true;
        $('#btnAi').textContent = text ? 'Interpret again' : 'Interpret results';
        ctx.emit('diagnosis:updated', {text, stale});
      }
    }
    $('#btnAi').addEventListener('click', run);
    $('#btnAiStop').addEventListener('click', () => { ctx.call('ai', 'cancel', {}).catch(() => {}); });

    // Provided to the what-if app: plain language -> slider factors, from the ai part.
    ctx.provide({
      async parseWhatIf({text: ask}){
        if (!available) throw new Error('AI is not available in this view.');
        const r = await ctx.call('ai', 'whatIf', {text: ask});
        if (r.error){ if (GONE.includes(r.error.code)) lose(); throw new Error(errCopy(r.error.code)); }
        return r;
      }
    });
  }
});

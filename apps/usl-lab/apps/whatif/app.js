/* whatif: "Try a change". Sliders for alpha, beta, lambda, plus a plain-language box (answered by diagnosis).
   Listens: analysis:ready, capability:ai.  Calls: diagnosis.parseWhatIf.  Emits: whatif:changed (retained). */
Kernel.register({
  name: 'whatif',
  emits: {'whatif:changed': {retain: true}},
  listens: ['analysis:ready', 'capability:ai'],
  needs: ['diagnosis.parseWhatIf'],
  init(ctx){
    const $ = ctx.$, {esc, fmt, peakInfo, whatIf, wiActive, describe} = Lib;
    let A = null, wi = {a: 1, b: 1, l: 1}, note = '';

    function publish(){ ctx.emit('whatif:changed', {a: wi.a, b: wi.b, l: wi.l, note}); render(); }

    function render(){
      $('#oa').textContent = '×' + wi.a.toFixed(2); $('#ob').textContent = '×' + wi.b.toFixed(2); $('#ol').textContent = '×' + wi.l.toFixed(2);
      const sum = $('#wiSum'), noteEl = $('#wiNote');
      $('#btnWiAsk').disabled = !(A && A.ok);
      if (!A || !A.ok){ sum.textContent = ''; noteEl.textContent = note; return; }
      const base = A.fit, u = A.units, pb = base.peak;
      if (!wiActive(wi)) sum.textContent = 'Currently: ' + describe(pb, u) + '.';
      else {
        const w = whatIf(base, wi), pw = peakInfo(w.lam, w.a, w.b);
        let delta = '';
        if (isFinite(pb.xMax) && isFinite(pw.xMax) && pb.xMax > 0){ const d = (pw.xMax/pb.xMax - 1)*100; delta = ' (' + (d >= 0 ? '+' : '') + d.toFixed(0) + '% best throughput)'; }
        sum.innerHTML = 'Now: ' + esc(describe(pb, u)) + '.<br>With these changes: <b>' + esc(describe(pw, u)) + '</b>' + esc(delta) + '.';
      }
      const hints = [];
      if (base.beta <= 1e-8) hints.push('β is not detectable in this data, so scaling it changes nothing.');
      if (base.alpha <= 1e-6) hints.push('α is about zero in this fit, so scaling it changes nothing.');
      noteEl.textContent = note || hints.join(' ');
    }

    ctx.on('analysis:ready', a => { A = a; render(); });
    ctx.on('capability:ai', c => { $('#wiAsk').hidden = !c.available; });

    [['#sa', 'a'], ['#sb', 'b'], ['#sl', 'l']].forEach(([id, k]) => $(id).addEventListener('input', e => { wi[k] = +e.target.value; note = ''; publish(); }));
    $('#btnWiReset').addEventListener('click', () => {
      wi = {a: 1, b: 1, l: 1}; note = '';
      $('#sa').value = 1; $('#sb').value = 1; $('#sl').value = 1; $('#wiText').value = '';
      publish();
    });

    async function ask(){
      const text = $('#wiText').value.trim(), btn = $('#btnWiAsk');
      if (!A || !A.ok || !text) return;
      btn.disabled = true; btn.textContent = 'Working…'; note = ''; $('#wiNote').textContent = '';
      try {
        const r = await ctx.call('diagnosis', 'parseWhatIf', {text});
        wi = {a: r.a, b: r.b, l: r.l}; note = r.rationale || '';
        $('#sa').value = wi.a; $('#sb').value = wi.b; $('#sl').value = wi.l;
        publish();
      } catch (e) { $('#wiNote').textContent = String(e && e.message || e); }
      finally { btn.disabled = false; btn.textContent = 'Apply to sliders'; }
    }
    $('#btnWiAsk').addEventListener('click', ask);
    $('#wiText').addEventListener('keydown', e => { if (e.key === 'Enter') ask(); });

    publish();
  }
});

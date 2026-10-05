/* inputs: the left column. Parses what you type, remembers it, publishes it.
   Emits: data:changed (retained), context:changed (retained).  Capabilities: storage. */
Kernel.register({
  name: 'inputs',
  emits: {'data:changed': {retain: true}, 'context:changed': {retain: true}},
  caps: ['storage'],
  init(ctx){
    const $ = ctx.$;

    const EXAMPLE = (() => {
      const Ns = [1, 2, 4, 8, 12, 16, 24, 32, 40, 48, 56, 64];
      const noise = [0.004, -0.012, 0.010, -0.008, 0.015, -0.006, -0.17, 0.012, -0.010, 0.009, -0.014, 0.011];
      const lines = ['threads, throughput (req/s), response time (ms)'];
      Ns.forEach((N, i) => {
        const X = 980*Lib.model(N, 0.04, 0.0006)*(1 + noise[i]);
        lines.push(N + ', ' + X.toFixed(0) + ', ' + (N/X*1000).toFixed(1));
      });
      return lines.join('\n');
    })();

    function parse(text){
      const rows = []; let skipped = 0;
      for (const raw of text.split(/\r?\n/)){
        const line = raw.trim(); if (!line) continue;
        const nums = line.split(/[,;\t ]+/).filter(Boolean).map(Number);
        if (isNaN(nums[0])){ if (rows.length || skipped) skipped++; else if (!/[a-z]/i.test(line)) skipped++; continue; }
        if (nums.length < 2 || !(nums[0] > 0) || !(nums[1] > 0) || !isFinite(nums[1])){ skipped++; continue; }
        rows.push({n: nums[0], x: nums[1], r: nums.length > 2 && nums[2] > 0 && isFinite(nums[2]) ? nums[2] : null});
      }
      rows.sort((p, q) => p.n - q.n);
      return {rows, skipped};
    }

    // restore (a blank saved state falls back to the example so the chart is never empty on first view)
    const saved = ctx.store.get('state');
    $('#data').value = saved && typeof saved.d === 'string' && saved.d.trim() ? saved.d : EXAMPLE;
    if (saved){
      $('#ctx').value = saved.c || ''; $('#nUnit').value = saved.n || 'threads'; $('#xUnit').value = saved.x || 'req/s';
      if (saved.r) $('#rUnit').value = saved.r;
    }

    const persist = () => ctx.store.set('state', {d: $('#data').value, c: $('#ctx').value, n: $('#nUnit').value, x: $('#xUnit').value, r: $('#rUnit').value});

    function publishData(){
      const {rows, skipped} = parse($('#data').value);
      const st = $('#parseStatus');
      st.className = 'status';
      if (!rows.length && !skipped) st.textContent = 'No runs yet.';
      else {
        st.textContent = rows.length + (rows.length === 1 ? ' run' : ' runs') + ' read' + (skipped ? '; ' + skipped + ' line' + (skipped === 1 ? '' : 's') + ' skipped (need positive numbers)' : '.');
        if (skipped) st.classList.add('warn');
      }
      const isExample = $('#data').value.trim() === EXAMPLE.trim();
      $('#exampleNote').hidden = !isExample;
      ctx.emit('data:changed', {
        rows, skipped, isExample,
        units: {n: $('#nUnit').value.trim() || 'units', x: $('#xUnit').value.trim() || 'per second'},
        rdiv: +$('#rUnit').value || 1
      });
    }
    const publishContext = () => ctx.emit('context:changed', {text: $('#ctx').value.trim()});

    const debounce = (fn, ms) => { let t = null; return () => { clearTimeout(t); t = setTimeout(() => { fn(); persist(); }, ms); }; };
    const dData = debounce(publishData, 280), dCtx = debounce(publishContext, 280);
    ['#data', '#nUnit', '#xUnit', '#rUnit'].forEach(id => { $(id).addEventListener('input', dData); $(id).addEventListener('change', dData); });
    $('#ctx').addEventListener('input', dCtx);

    $('#btnExample').addEventListener('click', () => { $('#data').value = EXAMPLE; publishData(); persist(); });
    $('#btnClear').addEventListener('click', () => { $('#data').value = ''; publishData(); persist(); $('#data').focus(); });
    $('#file').addEventListener('change', e => {
      const file = e.target.files && e.target.files[0]; if (!file) return;
      const fr = new FileReader();
      fr.onload = () => { $('#data').value = String(fr.result || ''); publishData(); persist(); };
      fr.readAsText(file); e.target.value = '';
    });
    $('#fileLbl').addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' '){ e.preventDefault(); $('#file').click(); } });

    publishData(); publishContext();
  }
});

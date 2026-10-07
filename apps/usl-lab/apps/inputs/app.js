/* inputs: the left column. Parses what you type, remembers it, publishes it.
   Emits: data:changed (retained), context:changed (retained).  Capabilities: storage.
   Channels: receives splunk.table, the latest table from the Splunk table app. You pick its columns. */
Kernel.register({
  name: 'inputs',
  emits: {'data:changed': {retain: true}, 'context:changed': {retain: true}},
  caps: ['storage'],
  channels: {receive: ['splunk.table']},
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

    // Where the measurements came from and what each column means, shown above the chart.
    // {title, load, throughput, response, method, range, at, edited}
    let source = null;

    // restore (a blank saved state falls back to the example so the chart is never empty on first view)
    const saved = ctx.store.get('state');
    $('#data').value = saved && typeof saved.d === 'string' && saved.d.trim() ? saved.d : EXAMPLE;
    if (saved){
      $('#ctx').value = saved.c || ''; $('#nUnit').value = saved.n || 'threads'; $('#xUnit').value = saved.x || 'req/s';
      if (saved.r) $('#rUnit').value = saved.r;
      if (saved.src && typeof saved.src === 'object') source = saved.src;
    }

    const persist = () => ctx.store.set('state', {d: $('#data').value, c: $('#ctx').value, n: $('#nUnit').value, x: $('#xUnit').value, r: $('#rUnit').value, src: source});

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
        rdiv: +$('#rUnit').value || 1,
        source: isExample ? EXAMPLE_SOURCE : source
      });
    }
    const publishContext = () => ctx.emit('context:changed', {text: $('#ctx').value.trim()});

    const EXAMPLE_SOURCE = {title: 'Example data (made up)', load: 'Threads in a load test', throughput: 'Requests per second at each thread count',
      response: 'Average response time, in milliseconds', method: 'A made-up load test, to show how the lab works. Replace it with your own runs or a table from the Splunk table app.'};
    // Typing in the box changes data that came from somewhere: say so, rather than keep a label that no longer fits.
    $('#data').addEventListener('input', () => { if (source && !source.edited){ source = Object.assign({}, source, {edited: true}); } });
    const debounce = (fn, ms) => { let t = null; return () => { clearTimeout(t); t = setTimeout(() => { fn(); persist(); }, ms); }; };
    const dData = debounce(publishData, 280), dCtx = debounce(publishContext, 280);
    ['#data', '#nUnit', '#xUnit', '#rUnit'].forEach(id => { $(id).addEventListener('input', dData); $(id).addEventListener('change', dData); });
    $('#ctx').addEventListener('input', dCtx);

    $('#btnExample').addEventListener('click', () => { $('#data').value = EXAMPLE; publishData(); persist(); });
    $('#btnClear').addEventListener('click', () => { $('#data').value = ''; source = null; publishData(); persist(); $('#data').focus(); });
    $('#file').addEventListener('change', e => {
      const file = e.target.files && e.target.files[0]; if (!file) return;
      const fr = new FileReader();
      fr.onload = () => { $('#data').value = String(fr.result || ''); source = {title: 'File: ' + file.name, method: 'Loaded from a file. The first column is the load, the second the throughput.'}; publishData(); persist(); };
      fr.readAsText(file); e.target.value = '';
    });
    $('#fileLbl').addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' '){ e.preventDefault(); $('#file').click(); } });

    // ---------- the Splunk table app: its latest table arrives on the channel "splunk.table" ----------
    // A cell can hold commas or spaces ("1,200"); keep only the number so the parser reads one value per column.
    const cell = v => {
      const t = v == null ? '' : String(v).replace(/[,\s]/g, ''), n = Number(t);
      return t !== '' && isFinite(n) ? String(+n.toPrecision(5)) : t;    // 7.885606060606062 -> 7.8856
    };
    function tblStatus(text, kind){ const st = $('#tblStatus'); st.className = 'status' + (kind ? ' ' + kind : ''); st.textContent = text; }
    let table = null, tableFrom = '';
    const NONE = '';
    function fill(sel, fields, pick, optional){
      sel.replaceChildren();
      if (optional) sel.appendChild(Object.assign(ctx.el('option'), {value: NONE, textContent: '(none)'}));
      for (const f of fields) sel.appendChild(Object.assign(ctx.el('option'), {value: f, textContent: f}));
      sel.value = fields.includes(pick) ? pick : (optional ? NONE : fields[0] || '');
    }
    function showTable(){
      const t = table, f = t.fields;
      $('#tblName').textContent = t.title + ': ' + t.rows.length + (t.rows.length === 1 ? ' row' : ' rows') + ', ' + (t.rangeLabel || 'all time') +
        ', fetched ' + new Date(t.at).toLocaleString() + (tableFrom ? ' by ' + tableFrom : '') + '.';
      const use = suggested(t);
      fill($('#colN'), f, use.load || f[0]);
      fill($('#colX'), f, use.throughput || f[1]);
      fill($('#colR'), f, use.response || (f.length > 2 ? f[2] : NONE), true);
      $('#tblPick').hidden = false; $('#btnUseTable').hidden = false;
      tblStatus(t.cut ? 'Only the first ' + t.rows.length + ' rows fit in one message from the table app.' : 'Check the columns, then press Use this table.', t.cut ? 'warn' : '');
    }
    // The columns the table app's labels describe. Without a list, labels describe the first three.
    const suggested = t => t.use || (t.about ? {load: t.fields[0], throughput: t.fields[1], response: t.fields[2] || ''} : {});
    const validTable = d => d && Array.isArray(d.fields) && Array.isArray(d.rows) && d.fields.every(x => typeof x === 'string') && d.rows.every(Array.isArray);
    async function connect(){
      $('#btnConnect').disabled = true;
      try {
        await ctx.channel('splunk.table').on((data, info) => {
          if (!validTable(data)){ tblStatus('A message on splunk.table was not a table, so it was left out.', 'warn'); return; }
          table = Object.assign({title: 'Splunk table'}, data); tableFrom = info && info.from || '';
          showTable();
        });
        ctx.store.set('tableLink', true);
        $('#btnConnect').hidden = true;
        if (!table) tblStatus('Waiting for a table. Run a search in the Splunk table app.');
      } catch (e) {
        ctx.store.set('tableLink', false);
        tblStatus('Cannot get tables: ' + (e && e.message || e) + '.', 'warn');
      } finally { $('#btnConnect').disabled = false; }
    }
    $('#btnConnect').addEventListener('click', connect);
    $('#btnUseTable').addEventListener('click', () => {
      if (!table) return;
      const f = table.fields, iN = f.indexOf($('#colN').value), iX = f.indexOf($('#colX').value), iR = f.indexOf($('#colR').value);
      if (iN < 0 || iX < 0){ tblStatus('Choose the load and the throughput columns.', 'warn'); return; }
      if (iN === iX){ tblStatus('Load and throughput must be different columns.', 'warn'); return; }
      const cols = [iN, iX].concat(iR >= 0 ? [iR] : []);
      const u = table.units || {};
      // Units from a ready-made search; otherwise keep what you typed.
      if (u.n) $('#nUnit').value = u.n;
      if (u.x) $('#xUnit').value = u.x;
      if (u.r) $('#rUnit').value = u.r === 's' ? '1' : '1000';
      const unitR = $('#rUnit').value === '1' ? 's' : 'ms';
      const header = '# ' + ($('#nUnit').value || f[iN]) + ', ' + ($('#xUnit').value || f[iX]) + (iR >= 0 ? ', response time (' + unitR + ')' : '');
      $('#data').value = [header].concat(table.rows.map(r => cols.map(i => cell(r[i])).join(', '))).join('\n');
      // The labels from the table app fit only the columns it suggested.
      const use = suggested(table), a = table.about || {};
      const same = use.load === f[iN] && use.throughput === f[iX] && (use.response || '') === (iR >= 0 ? f[iR] : '');
      const about = same && a.title ? a : {title: table.title, load: 'The column “' + f[iN] + '”', throughput: 'The column “' + f[iX] + '”',
        response: iR >= 0 ? 'The column “' + f[iR] + '”' : '', method: 'Each point is one row of the table.'};
      source = Object.assign({}, about, {title: about.title || table.title, range: table.rangeLabel, at: table.at, points: table.rows.length,
        search: String(table.search || '').slice(0, 2000),
        method: (about.method ? about.method + ' ' : '') + 'The rows came from the Splunk table app, where you can see every one.'});
      publishData(); persist();
      tblStatus('Using ' + table.rows.length + ' rows: load = ' + f[iN] + ', throughput = ' + f[iX] + (iR >= 0 ? ', response time = ' + f[iR] : '') + '.');
    });
    if (ctx.store.get('tableLink')) connect();
    else tblStatus('Press Get tables to receive tables from the Splunk table app. Wardian asks you first.');

    publishData(); publishContext();
  }
});

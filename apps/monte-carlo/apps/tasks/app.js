/* tasks: the left column. The task list with three estimates each, what each task waits for, and
   the simulation settings. Checks them, remembers them, and publishes them.
   A task's problem shows once you leave one of its fields (or press Run again), on just the fields it
   is about, with the message in a row under the task; it clears as soon as the task is right again.
   The simulation only ever runs a valid list.
   Emits: project:changed (retained: the current project).  Listens: sim:progress.
   Needs: sim.run (Run again), sim.cancel (Stop).  Capabilities: storage. */
Kernel.register({
  name: 'tasks',
  emits: {'project:changed': {retain: true}},
  listens: ['sim:progress'],
  needs: ['sim.run', 'sim.cancel'],
  caps: ['storage'],
  init(ctx){
    const $ = ctx.$, body = $('#rows'), bar = $('#bar');
    const newSeed = () => 1 + Math.floor(Math.random() * 999999);
    const DEFAULTS = {trials: 100000, dist: 'pert', unit: 'days', start: '', seed: newSeed()};
    const saved = ctx.store.get('project');
    let tasks = saved && Array.isArray(saved.tasks) ? saved.tasks : Project.sample();
    let settings = Object.assign({}, DEFAULTS, saved && saved.settings);
    let isSample = saved ? !!saved.sample : true;
    let timer = null;
    const shown = new Set();   // ids of tasks whose problems are on show

    // Task numbers are positions in the list; "after" keeps ids, so removing a task renumbers the rest.
    const numberOf = id => { const i = tasks.findIndex(t => t.id === id); return i < 0 ? String(id).replace(/^#/, '') : String(i + 1); };
    function parseAfter(text){
      return text.split(/[\s,;]+/).filter(Boolean).map(s => {
        const n = Number(s);
        return Number.isInteger(n) && n >= 1 && n <= tasks.length ? tasks[n - 1].id : '#' + s;   // '#…' is reported as missing
      }).filter((id, i, all) => all.indexOf(id) === i);
    }
    const nextId = () => 't' + (1 + tasks.reduce((m, t) => Math.max(m, Number(String(t.id).slice(1)) || 0), 0));

    function cell(cls, input){ const td = ctx.el('td'); if (cls) td.className = cls; td.appendChild(input); return td; }
    function field(type, value, label, cls){
      const i = ctx.el('input');
      i.className = 'w-input ' + cls; i.type = type; i.value = value == null ? '' : value; i.setAttribute('aria-label', label);
      if (type === 'number'){ i.classList.add('num'); i.min = '0'; i.step = 'any'; i.inputMode = 'decimal'; i.required = true; }
      else if (cls === 'tname') i.maxLength = 80;
      else i.maxLength = 40;
      return i;
    }
    function draw(){
      body.replaceChildren(...tasks.map((t, i) => {
        const tr = ctx.el('tr'), n = i + 1;
        tr.dataset.id = t.id;
        const num = ctx.el('td'); num.className = 'n'; num.textContent = n;
        const rm = ctx.el('button');
        rm.className = 'w-button'; rm.type = 'button'; rm.dataset.variant = 'ghost'; rm.dataset.size = 'icon';
        rm.textContent = '×'; rm.title = 'Remove task ' + n; rm.setAttribute('aria-label', 'Remove task ' + n);
        rm.addEventListener('click', () => remove(t.id));
        tr.append(num,
          cell('name', field('text', t.name, 'Task ' + n + ' name', 'tname')),
          cell('num', field('number', t.o, 'Task ' + n + ' best case', 'o')),
          cell('num', field('number', t.m, 'Task ' + n + ' most likely', 'm')),
          cell('num', field('number', t.p, 'Task ' + n + ' worst case', 'p')),
          cell('after', field('text', t.after.map(numberOf).join(', '), 'Task ' + n + ' waits for', 'aft')),
          cell('', rm));
        return tr;
      }));
      if (!tasks.length){
        const tr = ctx.el('tr'), td = ctx.el('td'); td.colSpan = 7; td.className = 'empty';
        td.textContent = 'No tasks yet. Press Add task to start one, or Sample project to see how it works.';
        tr.appendChild(td); body.appendChild(tr);
      }
    }
    const taskRows = () => Array.from(body.querySelectorAll('tr[data-id]'));
    // Reads the table back into `tasks`, through the same parsers the saved list uses.
    function read(){
      const dur = x => { const r = Project.parseDuration(x.value); return r.error ? NaN : r.value; };
      taskRows().forEach((tr, i) => {
        const t = tasks[i]; if (!t) return;
        t.name = tr.querySelector('.tname').value.trim();
        t.o = dur(tr.querySelector('.o')); t.m = dur(tr.querySelector('.m')); t.p = dur(tr.querySelector('.p'));
        t.after = parseAfter(tr.querySelector('.aft').value);
      });
      const seed = Project.parseSeed($('#seed').value);
      settings = {
        trials: Number($('#trials').value), dist: $('#dist').value, unit: $('#unit').value, start: $('#start').value,
        seed: seed.error ? settings.seed : seed.value,   // a bad seed is reported, and the last good one stays in use
      };
      return seed;
    }
    function markSeed(seed){
      const input = $('#seed'), hint = $('#seedHint');
      input.setAttribute('aria-invalid', seed.error ? 'true' : 'false');
      hint.textContent = seed.error || 'A whole number from 1 to 2,147,483,647. The same seed and tasks always give the same results.';
    }
    function publish(){
      const chk = Project.check(tasks);
      body.querySelectorAll('tr.err').forEach(r => r.remove());
      taskRows().forEach((tr, i) => {
        const t = tasks[i], msg = chk.rows[i];
        if (!msg) shown.delete(t.id);
        const visible = !!msg && shown.has(t.id), bad = visible ? Project.fieldsOf(t, msg) : [];
        tr.classList.toggle('bad', visible);
        const errId = 'err-' + t.id;
        tr.querySelectorAll('input').forEach(x => {
          const k = ['o', 'm', 'p', 'aft', 'tname'].find(c => x.classList.contains(c));
          const on = bad.includes(k);
          x.setAttribute('aria-invalid', on ? 'true' : 'false');
          if (on) x.setAttribute('aria-describedby', errId); else x.removeAttribute('aria-describedby');
        });
        if (visible){
          const er = ctx.el('tr'), td = ctx.el('td');
          er.className = 'err'; td.colSpan = 7; td.id = errId; td.textContent = msg;
          er.appendChild(td); tr.after(er);
        }
      });
      const anyShown = chk.rows.some((m, i) => m && shown.has(tasks[i].id));
      $('#problem').textContent = !tasks.length || tasks.length > Project.MAX_TASKS ? chk.problem
        : anyShown ? 'Fix the highlighted fields to run the simulation.' : '';
      $('#sampleBadge').hidden = !isSample;
      const clean = tasks.map((t, i) => ({id: t.id, name: t.name || 'Task ' + (i + 1), o: t.o, m: t.m, p: t.p, after: t.after}));
      ctx.emit('project:changed', {tasks: clean, valid: chk.ok, problem: chk.problem, settings});
      ctx.store.set('project', {tasks: tasks.map(t => Object.assign({}, t, {o: num(t.o), m: num(t.m), p: num(t.p)})), settings, sample: isSample});
      return chk;
    }
    const num = v => Number.isFinite(v) ? v : null;
    const edited = () => { isSample = false; };
    function changed(){ clearTimeout(timer); timer = setTimeout(() => { read(); publish(); }, 200); }
    function showAll(){ tasks.forEach(t => shown.add(t.id)); }
    function focusFirstError(){
      const x = body.querySelector('input[aria-invalid="true"]') || ($('#seed').getAttribute('aria-invalid') === 'true' ? $('#seed') : null);
      if (x) x.focus();
      return !!x;
    }
    const ask = (text, opts) => (window.WardianUI && WardianUI.confirm) ? WardianUI.confirm(text, opts) : Promise.resolve(window.confirm(text));

    async function remove(id){
      read();
      tasks = tasks.filter(t => t.id !== id); shown.delete(id); edited();
      tasks.forEach(t => { t.after = t.after.filter(a => a !== id); });
      draw(); publish();
      $('#add').focus();
    }
    $('#add').addEventListener('click', () => {
      read(); edited();
      const last = tasks[tasks.length - 1];
      tasks.push({id: nextId(), name: '', o: 1, m: 2, p: 4, after: last ? [last.id] : []});
      draw(); publish();
      const rows = taskRows(); rows[rows.length - 1].querySelector('.tname').focus();
    });
    $('#sample').addEventListener('click', async () => {
      if (tasks.length && !isSample && !(await ask('Your tasks will be replaced by the sample project.', {title: 'Replace your tasks?', ok: 'Replace', destructive: true}))) return;
      tasks = Project.sample(); isSample = true; shown.clear(); draw(); read(); publish();
    });
    $('#clear').addEventListener('click', async () => {
      if (tasks.length && !(await ask('Every task will be removed. This cannot be undone.', {title: 'Remove every task?', ok: 'Remove all', destructive: true}))) return;
      tasks = []; isSample = false; shown.clear(); draw(); read(); publish();
      $('#add').focus();
    });
    $('#newSeed').addEventListener('click', () => { $('#seed').value = newSeed(); markSeed(read()); publish(); });
    $('#run').addEventListener('click', () => {
      const seed = read(); markSeed(seed); showAll();
      const chk = publish();
      if (!chk.ok || seed.error){ focusFirstError(); return; }
      ctx.call('sim', 'run', {}).catch(e => { $('#problem').textContent = 'Could not start the run: ' + Fmt.errText(e); });
    });
    // Typing re-checks (and clears a fixed problem) without showing new ones; leaving a field shows them.
    body.addEventListener('input', () => { edited(); changed(); });
    body.addEventListener('change', e => {
      const tr = e.target.closest('tr[data-id]');
      if (tr) shown.add(tr.dataset.id);
      clearTimeout(timer); read(); publish();
    });
    ['trials', 'dist', 'unit', 'start'].forEach(id => $('#' + id).addEventListener('change', () => { read(); publish(); }));
    $('#seed').addEventListener('input', () => { const s = Project.parseSeed($('#seed').value); if (!s.error) markSeed(s); });
    $('#seed').addEventListener('change', () => { markSeed(read()); publish(); });

    SimBar.attach(ctx, bar, t => { $('#problem').textContent = t; });
    ctx.on('sim:progress', p => SimBar.show(bar, p));

    for (const k of ['trials', 'dist', 'unit', 'start', 'seed']) $('#' + k).value = settings[k];
    draw(); publish();
  }
});

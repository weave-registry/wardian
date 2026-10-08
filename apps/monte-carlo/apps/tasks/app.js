/* tasks: the left column. The task list with three estimates each, what each task waits for, and
   the simulation settings. Checks them as you type, remembers them, and publishes them.
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
    let timer = null;

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
      i.className = 'w-input ' + (cls || ''); i.type = type; i.value = value == null ? '' : value; i.setAttribute('aria-label', label);
      if (type === 'number'){ i.min = '0'; i.step = 'any'; i.inputMode = 'decimal'; }
      return i;
    }
    function draw(){
      body.replaceChildren(...tasks.map((t, i) => {
        const tr = ctx.el('tr'), n = i + 1;
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
        td.textContent = 'No tasks. Press Add task, or Sample project to see how it works.';
        tr.appendChild(td); body.appendChild(tr);
      }
    }
    // Reads the table back into `tasks`. Numbers stay as typed until they are valid.
    function read(){
      const v = x => x.value.trim() === '' ? NaN : Number(x.value);
      Array.from(body.querySelectorAll('tr')).forEach((tr, i) => {
        const t = tasks[i]; if (!t) return;
        t.name = tr.querySelector('.tname').value.trim();
        t.o = v(tr.querySelector('.o')); t.m = v(tr.querySelector('.m')); t.p = v(tr.querySelector('.p'));
        t.after = parseAfter(tr.querySelector('.aft').value);
      });
      settings = {
        trials: Number($('#trials').value), dist: $('#dist').value, unit: $('#unit').value, start: $('#start').value,
        seed: Math.max(1, Math.min(2147483647, Math.round(Number($('#seed').value)) || settings.seed)),
      };
    }
    function publish(){
      const chk = Project.check(tasks);
      Array.from(body.querySelectorAll('tr')).forEach((tr, i) => {
        tr.classList.toggle('bad', !!chk.rows[i]);
        tr.title = chk.rows[i] || '';
        tr.querySelectorAll('input').forEach(x => x.setAttribute('aria-invalid', chk.rows[i] ? 'true' : 'false'));
      });
      $('#problem').textContent = chk.problem;
      const clean = tasks.map((t, i) => ({id: t.id, name: t.name || 'Task ' + (i + 1), o: t.o, m: t.m, p: t.p, after: t.after}));
      ctx.emit('project:changed', {tasks: clean, valid: chk.ok, problem: chk.problem, settings});
      ctx.store.set('project', {tasks: tasks.map(t => Object.assign({}, t, {o: num(t.o), m: num(t.m), p: num(t.p)})), settings});
    }
    const num = v => Number.isFinite(v) ? v : null;
    function changed(){ clearTimeout(timer); timer = setTimeout(() => { read(); publish(); }, 200); }

    function remove(id){
      read();
      tasks = tasks.filter(t => t.id !== id);
      tasks.forEach(t => { t.after = t.after.filter(a => a !== id); });
      draw(); publish();
    }
    $('#add').addEventListener('click', () => {
      read();
      const last = tasks[tasks.length - 1];
      tasks.push({id: nextId(), name: '', o: 1, m: 2, p: 4, after: last ? [last.id] : []});
      draw(); publish();
      const rows = body.querySelectorAll('tr'); rows[rows.length - 1].querySelector('.tname').focus();
    });
    $('#sample').addEventListener('click', () => {
      if (tasks.length && !window.confirm('Replace your tasks with the sample project?')) return;
      tasks = Project.sample(); draw(); read(); publish();
    });
    $('#clear').addEventListener('click', () => {
      if (tasks.length && !window.confirm('Remove every task?')) return;
      tasks = []; draw(); read(); publish();
    });
    $('#newSeed').addEventListener('click', () => { $('#seed').value = newSeed(); read(); publish(); });
    $('#run').addEventListener('click', () => {
      read();
      ctx.call('sim', 'run', {}).catch(e => { $('#problem').textContent = 'Could not start the run: ' + Fmt.errText(e); });
    });
    body.addEventListener('input', changed);
    ['trials', 'dist', 'unit', 'start'].forEach(id => $('#' + id).addEventListener('change', () => { read(); publish(); }));
    $('#seed').addEventListener('change', () => { read(); $('#seed').value = settings.seed; publish(); });

    SimBar.attach(ctx, bar, t => { $('#problem').textContent = t; });
    ctx.on('sim:progress', p => SimBar.show(bar, p));

    for (const k of ['trials', 'dist', 'unit', 'start', 'seed']) $('#' + k).value = settings[k];
    draw(); publish();
  }
});

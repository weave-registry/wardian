/* habits: the left column. Adds, renames, recolours, reorders and deletes habits, and remembers them.
   It owns the list of habits; the log part owns the check-ins, so deleting a habit asks log.forget.
   Emits: habits:changed (retained: { habits }).  Listens: log:changed (for the counts).
   Needs: log.forget.  Provides: restore (used by backup).  Capabilities: storage. */
Kernel.register({
  name: 'habits',
  emits: { 'habits:changed': { retain: true } },
  listens: ['log:changed'],
  needs: ['log.forget'],
  provides: ['restore'],
  caps: ['storage'],
  init(ctx) {
    const $ = ctx.$, note = $('#note');
    let habits = [];
    try { habits = Model.habits(ctx.store.get('habits') || []); } catch { habits = []; }
    let days = {}, editing = null, deleting = null;

    const say = (text, warn) => { note.textContent = text; note.classList.toggle('warn', !!warn); };
    const save = () => { ctx.store.set('habits', habits); ctx.emit('habits:changed', { habits }); render(); };
    const nextColor = () => (Model.COLORS.find(c => !habits.some(h => h.color === c[0])) || Model.COLORS[habits.length % Model.COLORS.length])[0];

    // A row of colour choices: real radio buttons, so arrow keys move between them.
    function palette(group, selected) {
      const box = ctx.el('div');
      box.className = 'palette';
      for (const [hex, label] of Model.COLORS) {
        const l = ctx.el('label'), r = ctx.el('input'), dot = ctx.el('span'), sr = ctx.el('span');
        r.type = 'radio'; r.name = group; r.value = hex; r.checked = hex === selected;
        dot.className = 'dot'; dot.style.background = hex;
        sr.className = 'sr'; sr.textContent = label;
        l.title = label; l.append(r, dot, sr); box.append(l);
      }
      return box;
    }
    const chosen = (root, group) => (root.querySelector('input[name="' + group + '"]:checked') || {}).value || Model.COLORS[0][0];
    const resetAdd = () => { $('#palette').replaceChildren(palette('newColor', nextColor())); };

    function button(text, act, id, label, variant = 'ghost') {
      const b = ctx.el('button');
      b.type = 'button'; b.className = 'w-button'; b.dataset.variant = variant; b.dataset.size = 'sm';
      b.textContent = text; b.dataset.act = act; b.dataset.id = id;
      if (label) b.setAttribute('aria-label', label);
      return b;
    }

    function render(focus) {
      const list = $('#list');
      list.replaceChildren();
      $('#empty').hidden = habits.length > 0;
      habits.forEach((h, i) => {
        const li = ctx.el('li'), n = (days[h.id] || []).length;
        li.className = 'habit';
        if (editing === h.id) {
          const f = ctx.el('form'), input = ctx.el('input');
          f.className = 'edit'; f.dataset.id = h.id;
          input.className = 'w-input'; input.value = h.name; input.maxLength = Model.MAX_NAME;
          input.setAttribute('aria-label', 'Name of ' + h.name);
          const row = ctx.el('div');
          row.className = 'btnrow';
          row.append(button('Save', 'save', h.id, null, 'default'), button('Cancel', 'cancel', h.id, null, 'outline'));
          f.append(input, palette('c-' + h.id, h.color), row);
          li.append(f);
          list.append(li);
          if (focus !== false) input.focus();
          return;
        }
        if (deleting === h.id) {
          const p = ctx.el('p');
          p.className = 'confirm';
          p.textContent = 'Delete “' + h.name + '”' + (n ? ' and its ' + Stats.plural(n, 'check-in') + '?' : '?');
          const row = ctx.el('div');
          row.className = 'btnrow';
          row.append(button('Delete', 'really', h.id, null, 'destructive'), button('Keep it', 'keep', h.id, null, 'outline'));
          li.append(p, row);
          list.append(li);
          row.firstChild.focus();
          return;
        }
        const dot = ctx.el('span'), name = ctx.el('span'), count = ctx.el('span'), tools = ctx.el('span');
        dot.className = 'dot'; dot.style.background = h.color;
        name.className = 'name'; name.textContent = h.name;
        count.className = 'count'; count.textContent = n ? Stats.plural(n, 'day') : '';
        tools.className = 'tools';
        const up = button('↑', 'up', h.id, 'Move ' + h.name + ' up'), down = button('↓', 'down', h.id, 'Move ' + h.name + ' down');
        up.disabled = i === 0; down.disabled = i === habits.length - 1;
        tools.append(up, down, button('Edit', 'edit', h.id, 'Edit ' + h.name), button('Delete', 'delete', h.id, 'Delete ' + h.name));
        li.append(dot, name, count, tools);
        list.append(li);
      });
      if (focus) { const b = list.querySelector('[data-act="' + focus.act + '"][data-id="' + focus.id + '"]'); if (b && !b.disabled) b.focus(); }
    }

    function add(name, color) {
      name = name.trim();
      if (!name) { say('Give the habit a name.', true); $('#name').focus(); return false; }
      if (habits.length >= Model.MAX_HABITS) { say('You can keep up to ' + Model.MAX_HABITS + ' habits.', true); return false; }
      if (habits.some(h => h.name.toLowerCase() === name.toLowerCase())) { say('You already have “' + name + '”.', true); return false; }
      habits = [...habits, { id: Model.newId(), name: name.slice(0, Model.MAX_NAME), color, created: Days.today() }];
      say('Added “' + name + '”.');
      save();
      return true;
    }

    $('#add').addEventListener('submit', e => {
      e.preventDefault();
      if (add($('#name').value, chosen($('#add'), 'newColor'))) { $('#name').value = ''; resetAdd(); $('#name').focus(); }
    });
    $('#examples').addEventListener('click', () => {
      [['Read 20 pages', 1], ['Walk 30 minutes', 0], ['No phone after 10pm', 2]]
        .forEach(([name, c]) => add(name, Model.COLORS[c][0]));
      say('Added three examples. Edit or delete them as you like.');
      resetAdd();
    });

    $('#list').addEventListener('submit', e => {      // Enter in the rename box saves
      e.preventDefault();
      act('save', e.target.dataset.id);
    });
    $('#list').addEventListener('click', e => {
      const b = e.target.closest('button[data-act]');
      if (b) act(b.dataset.act, b.dataset.id);
    });
    $('#list').addEventListener('keydown', e => {
      if (e.key === 'Escape' && (editing || deleting)) { const id = editing || deleting; editing = deleting = null; render({ act: 'edit', id }); }
    });

    async function act(what, id) {
      const i = habits.findIndex(h => h.id === id), h = habits[i];
      if (!h) return;
      if (what === 'up' || what === 'down') {
        const j = what === 'up' ? i - 1 : i + 1;
        if (j < 0 || j >= habits.length) return;
        const next = [...habits];
        [next[i], next[j]] = [next[j], next[i]];
        habits = next;
        ctx.store.set('habits', habits); ctx.emit('habits:changed', { habits });
        render({ act: what, id });
      } else if (what === 'edit') { editing = id; deleting = null; render(); }
      else if (what === 'cancel') { editing = null; render({ act: 'edit', id }); }
      else if (what === 'save') {
        const f = $('#list form.edit'), name = f.querySelector('input.w-input').value.trim();
        if (!name) { say('A habit needs a name.', true); return; }
        if (habits.some(o => o.id !== id && o.name.toLowerCase() === name.toLowerCase())) { say('You already have “' + name + '”.', true); return; }
        habits = habits.map(o => o.id === id ? { ...o, name: name.slice(0, Model.MAX_NAME), color: chosen(f, 'c-' + id) } : o);
        editing = null; say('Saved “' + name + '”.');
        ctx.store.set('habits', habits); ctx.emit('habits:changed', { habits });
        render({ act: 'edit', id });
      } else if (what === 'delete') { deleting = id; editing = null; render(); }
      else if (what === 'keep') { deleting = null; render({ act: 'delete', id }); }
      else if (what === 'really') {
        deleting = null;
        habits = habits.filter(o => o.id !== id);
        save();
        try { await ctx.call('log', 'forget', { habit: id }); say('Deleted “' + h.name + '”.'); }
        catch (e) { say('Deleted “' + h.name + '”, but its check-ins could not be removed: ' + e.message, true); }
        $('#name').focus();
      }
    }

    ctx.provide({
      /** Replaces every habit, from a backup. */
      restore({ habits: incoming } = {}) {
        habits = Model.habits(incoming);
        editing = deleting = null;
        save(); resetAdd();
        return { habits: habits.length };
      },
    });
    ctx.on('log:changed', p => { days = p.days; if (!editing && !deleting) render(false); });

    resetAdd();
    render(false);
    ctx.emit('habits:changed', { habits });
  }
});

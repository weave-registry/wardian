/* actions: the action items as a checklist the viewer owns. It starts from each new reading, then
   keeps the viewer's ticks and edits (in storage) and publishes the list for export.
   Listens: notes:extracted.  Emits: actions:changed (retained: { title, items }).  Capabilities: storage. */
Kernel.register({
  name: 'actions',
  listens: ['notes:extracted'],
  emits: { 'actions:changed': { retain: true } },
  caps: ['storage'],
  init(ctx) {
    const $ = ctx.$;
    const saved = ctx.store.get('list') || {};
    let from = saved.from || null, title = saved.title || '', items = Array.isArray(saved.items) ? saved.items : [];
    const newId = () => 'a' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
    const key = s => String(s).toLowerCase().replace(/\W+/g, ' ').trim();

    function save(redraw = true) {
      ctx.store.set('list', { from, title, items });
      ctx.emit('actions:changed', { title, items });
      if (redraw) render(); else count();
    }
    function count() {
      const done = items.filter(i => i.done).length;
      $('#sub').textContent = items.length ? done + ' of ' + items.length + ' done' : '';
      $('#empty').hidden = items.length > 0;
    }

    function field(item, name, label, placeholder) {
      const input = ctx.el('input');
      input.className = 'cell ' + name; input.value = item[name] || ''; input.placeholder = placeholder;
      input.setAttribute('aria-label', label); input.dataset.field = name; input.maxLength = name === 'task' ? 500 : 80;
      return input;
    }
    function render(focusId) {
      const list = $('#list');
      list.replaceChildren();
      items.forEach((it, i) => {
        const li = ctx.el('li'), box = ctx.el('input'), x = ctx.el('button'), meta = ctx.el('div');
        li.className = 'item' + (it.done ? ' done' : ''); li.dataset.id = it.id;
        box.type = 'checkbox'; box.checked = !!it.done; box.className = 'check';
        box.setAttribute('aria-label', 'Done: ' + (it.task || 'item ' + (i + 1)));
        x.type = 'button'; x.className = 'w-button remove'; x.dataset.variant = 'ghost'; x.dataset.size = 'sm';
        x.textContent = '×'; x.setAttribute('aria-label', 'Remove: ' + (it.task || 'item ' + (i + 1)));
        meta.className = 'meta';
        meta.append(field(it, 'owner', 'Owner of item ' + (i + 1), 'Owner'), field(it, 'due', 'Due date of item ' + (i + 1), 'Due'));
        const main = ctx.el('div');
        main.className = 'main';
        main.append(field(it, 'task', 'Item ' + (i + 1), 'What needs doing'), meta);
        li.append(box, main, x);
        list.append(li);
      });
      count();
      if (focusId) { const el = list.querySelector('[data-id="' + focusId + '"] input.task'); if (el) el.focus(); }
    }

    const itemOf = el => { const li = el.closest('li.item'); return li ? items.find(i => i.id === li.dataset.id) : null; };
    $('#list').addEventListener('change', e => {
      const it = itemOf(e.target);
      if (!it || !e.target.classList.contains('check')) return;
      items = items.map(i => i === it ? { ...i, done: e.target.checked } : i);
      e.target.closest('li').classList.toggle('done', e.target.checked);
      save(false);
    });
    let timer = null;
    $('#list').addEventListener('input', e => {
      const it = itemOf(e.target), f = e.target.dataset.field;
      if (!it || !f) return;
      items = items.map(i => i === it ? { ...i, [f]: e.target.value, edited: true } : i);
      clearTimeout(timer); timer = setTimeout(() => save(false), 300);
    });
    $('#list').addEventListener('click', e => {
      if (!e.target.classList.contains('remove')) return;
      const it = itemOf(e.target), at = items.indexOf(it);
      items = items.filter(i => i !== it);
      save();
      const next = $('#list').children[Math.min(at, items.length - 1)];
      (next ? next.querySelector('.remove') : $('#add')).focus();
    });
    $('#add').addEventListener('click', () => {
      const it = { id: newId(), task: '', owner: '', due: '', done: false, added: true };
      items = [...items, it];
      save(false); render(it.id);
    });

    ctx.on('notes:extracted', r => {
      if (r.id === from || r.stale) { title = r.title || title; render(); ctx.emit('actions:changed', { title, items }); return; }
      // A new reading: its items replace the old ones, but ticks carry over to items with the same
      // task, and items the viewer added stay.
      const before = new Map(items.filter(i => !i.added).map(i => [key(i.task), i]));
      const fresh = r.empty ? [] : r.action_items.map(a => {
        const old = before.get(key(a.task));
        return old ? (old.edited ? old : { ...old, owner: a.owner, due: a.due }) : { id: newId(), task: a.task, owner: a.owner, due: a.due, done: !!a.done };
      });
      items = r.empty ? [] : [...fresh, ...items.filter(i => i.added && i.task.trim())];
      from = r.id; title = r.title || '';
      save();
    });
    render();
  }
});

/* summary: what the meeting was about, what it decided, and what it left open. Read-only: it draws
   the latest reading and writes nothing.
   Listens: notes:extracted. */
Kernel.register({
  name: 'summary',
  listens: ['notes:extracted'],
  init(ctx) {
    const $ = ctx.$;
    function fill(list, items, none) {
      list.replaceChildren(...(items.length ? items : [none]).map(t => {
        const li = ctx.el('li');
        li.textContent = t;
        if (!items.length) li.className = 'none';
        return li;
      }));
    }
    ctx.on('notes:extracted', r => {
      const empty = !!r.empty;
      $('#empty').hidden = !empty;
      $('#body').hidden = empty;
      $('#source').hidden = empty;
      $('#title').textContent = empty ? 'Summary' : r.title;
      if (empty) return;
      const src = $('#source');
      src.textContent = r.source === 'claude' ? 'Read by Claude' : 'Quick reader';
      src.dataset.variant = r.source === 'claude' ? 'success' : 'outline';
      src.title = 'Read ' + new Date(r.at).toLocaleString() + ', ' + Text.plural(r.words || 0, 'word');
      $('#stale').hidden = !r.stale;
      $('#fallback').hidden = !r.note;
      $('#fallback').textContent = r.note || '';
      $('#summary').textContent = r.summary || 'No summary.';
      $('#dcount').textContent = r.decisions.length ? '(' + r.decisions.length + ')' : '';
      $('#qcount').textContent = r.open_questions.length ? '(' + r.open_questions.length + ')' : '';
      fill($('#decisions'), r.decisions, 'No decisions found.');
      fill($('#questions'), r.open_questions, 'No open questions found.');
    });
  }
});

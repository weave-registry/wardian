/* ai: the only part that reads the notes. It owns every call to Claude (claude:sample) and the
   quick reader that works without AI, and publishes one reading for the views.
   Without a Claude provider, ctx.cap('sample') is null: the quick reader then reads the notes as
   they change, and the panel says how to turn AI on.
   Listens: notes:changed.  Emits: notes:extracted (retained).  Capabilities: storage, claude:sample. */
Kernel.register({
  name: 'ai',
  listens: ['notes:changed'],
  emits: { 'notes:extracted': { retain: true } },
  caps: ['storage', 'claude:sample'],
  init(ctx) {
    const $ = ctx.$, bar = $('#bar');
    let notes = null, sample = null, ready = false, controller = null;
    let last = ctx.store.get('last');                  // the latest reading, so a reload shows it again

    const status = (text, warn) => { $('#status').textContent = text || ''; $('#status').classList.toggle('warn', !!warn); };
    const words = t => (t.match(/\S+/g) || []).length;
    function publish(reading) {
      last = reading;
      ctx.store.set('last', reading);
      ctx.emit('notes:extracted', reading);
    }
    // Wraps a reading with what the views need to know about it.
    const wrap = (source, r, n, extra = {}) => ({ ...r, ...extra, source, title: n.title || 'Meeting notes', hash: n.hash,
      id: n.hash + '-' + source + '-' + Date.now().toString(36), at: new Date().toISOString(), words: words(n.text) });

    function readLocally(note) {
      if (!notes || !notes.text.trim()) return;
      publish(wrap('local', LocalReader.read(notes.title, notes.text), notes, note ? { note } : {}));
    }

    async function readWithClaude() {
      if (!sample || controller || !notes) return;
      const n = notes;
      if (!n.text.trim()) { status('Paste or open some notes first.', true); return; }
      controller = new AbortController();
      $('#claude').disabled = $('#local').disabled = true;
      status('');
      bar.start('Claude is reading ' + Text.plural(words(n.text), 'word'), { cancelable: true,
        detail: 'This usually takes 10 to 40 seconds. You can keep working in the other panels.' });
      try {
        const r = await ClaudeReader.read(sample, n.title, n.text, controller.signal);
        bar.update({ detail: '' }).done('Claude read the notes in ' + bar.seconds + ' s');
        publish(wrap('claude', r, n));
        if (notes.hash !== n.hash) status('The notes changed while Claude read them. Press Read with Claude again to include the changes.');
      } catch (e) {
        const { code, text } = ClaudeReader.explain(e);
        const haveClaude = last && last.source === 'claude' && last.hash === n.hash;
        bar.update({ detail: '' });
        if (code === 'cancelled') { bar.fail('Stopped'); status(text); }
        else {
          bar.fail('Claude could not read the notes');
          status(text + (haveClaude ? ' The earlier reading by Claude stays.' : ' The quick reader’s results are shown instead.'), true);
          if (!haveClaude) readLocally('Claude could not read these notes (' + code + '), so the quick reader did.');
        }
      } finally {
        controller = null;
        $('#claude').disabled = $('#local').disabled = false;
      }
    }

    bar.addEventListener('cancel', () => { if (controller) controller.abort(); });
    $('#claude').addEventListener('click', readWithClaude);
    $('#local').addEventListener('click', () => { readLocally(); status('The quick reader read the notes.'); });

    function onNotes(p) {
      notes = p;
      if (!ready) return;                              // wait until we know whether Claude is there
      if (!p.text.trim()) {
        if (last) { last = null; ctx.store.set('last', null); }
        ctx.emit('notes:extracted', { empty: true, source: 'none', id: 'empty', hash: p.hash, title: p.title || 'Meeting notes',
          summary: '', decisions: [], action_items: [], open_questions: [] });
        return;
      }
      if (last && last.hash === p.hash) { ctx.emit('notes:extracted', last); return; }
      if (!last || last.source !== 'claude' || !sample) { readLocally(); return; }
      // A reading by Claude of older notes: keep it, marked as out of date, until the viewer asks again.
      ctx.emit('notes:extracted', { ...last, stale: true });
      status('The notes changed since Claude read them. Press Read with Claude to read them again.');
    }
    ctx.on('notes:changed', onNotes);

    (async () => {
      sample = await ctx.cap('sample').catch(() => null);
      const mode = $('#mode');
      if (sample) {
        mode.textContent = 'Claude is on'; mode.dataset.variant = 'success';
        $('#about').textContent = 'Claude writes a summary and finds the decisions, action items and open questions. ' +
          'Your notes go to Claude, through Wardian, only when you press the button. Until then the quick reader shows what it finds.';
        $('#claude').hidden = false;
      } else {
        mode.textContent = 'AI is off'; mode.dataset.variant = 'secondary';
        $('#about').textContent = 'No Claude provider is set up in this Wardian, so the quick reader reads your notes here, as you type. ' +
          'It finds lines such as “Action: …”, “TODO …”, “@sam will …”, “Decision: …”, check boxes, and questions. It does not write a summary.';
        $('#local').hidden = true;
        $('#howto').hidden = false;
      }
      ready = true;
      if (notes) onNotes(notes);
    })();
  }
});

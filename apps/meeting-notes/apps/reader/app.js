/* reader: the Reader panel. Shows whether Claude is on, reads the notes with the quick reader as they
   change, and asks the out-of-sight ai part to read them with Claude when the viewer presses the
   button; then publishes one reading for the views.
   Listens: notes:changed.  Emits: notes:extracted (retained).  Needs: ai.status, ai.read, ai.cancel.
   Capabilities: storage. */
Kernel.register({
  name: 'reader',
  listens: ['notes:changed'],
  emits: { 'notes:extracted': { retain: true } },
  needs: ['ai.status', 'ai.read', 'ai.cancel'],
  caps: ['storage'],
  init(ctx) {
    const $ = ctx.$, bar = $('#bar');
    let notes = null, claude = false, ready = false, reading = false;
    let last = ctx.store.get('last');                  // the latest reading, so a reload shows it again

    const status = (text, warn) => { $('#status').textContent = text || ''; $('#status').classList.toggle('warn', !!warn); };
    const words = t => (t.match(/\S+/g) || []).length;
    function publish(r) {
      last = r;
      ctx.store.set('last', r);
      ctx.emit('notes:extracted', r);
    }
    // Wraps a reading with what the views need to know about it.
    const wrap = (source, r, n, extra = {}) => ({ ...r, ...extra, source, title: n.title || 'Meeting notes', hash: n.hash,
      id: n.hash + '-' + source + '-' + Date.now().toString(36), at: new Date().toISOString(), words: words(n.text) });

    function readLocally(note) {
      if (!notes || !notes.text.trim()) return;
      publish(wrap('local', LocalReader.read(notes.title, notes.text), notes, note ? { note } : {}));
    }

    async function readWithClaude() {
      if (!claude || reading || !notes) return;
      const n = notes;
      if (!n.text.trim()) { status(Words.explain({ code: 'empty' }).text, true); return; }
      reading = true;
      $('#claude').disabled = $('#local').disabled = true;
      status('');
      bar.start('Claude is reading ' + Text.plural(words(n.text), 'word'), { cancelable: true,
        detail: 'This usually takes 10 to 40 seconds. You can keep working in the other panels.' });
      try {
        const out = await ctx.call('ai', 'read', {});
        if (out && out.reading) {
          bar.update({ detail: '' }).done('Claude read the notes in ' + bar.seconds + ' s');
          publish(wrap('claude', out.reading, n));
          if (notes.hash !== n.hash) status('The notes changed while Claude read them. Press Read with Claude again to include the changes.');
          return;
        }
        throw out && out.error ? out.error : { code: 'error', message: 'no answer' };
      } catch (e) {
        const { code, text } = Words.explain(e);
        const haveClaude = last && last.source === 'claude' && last.hash === n.hash;
        bar.update({ detail: '' });
        if (code === 'cancelled') { bar.fail('Stopped'); status(text); }
        else {
          bar.fail('Claude could not read the notes');
          status(text + (haveClaude ? ' The earlier reading by Claude stays.' : ' The quick reader’s results are shown instead.'), true);
          if (!haveClaude) readLocally('Claude could not read these notes (' + code + '), so the quick reader did.');
        }
      } finally {
        reading = false;
        $('#claude').disabled = $('#local').disabled = false;
      }
    }

    bar.addEventListener('cancel', () => { ctx.call('ai', 'cancel', {}).catch(() => {}); });
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
      if (!last || last.source !== 'claude' || !claude) { readLocally(); return; }
      // A reading by Claude of older notes: keep it, marked as out of date, until the viewer asks again.
      ctx.emit('notes:extracted', { ...last, stale: true });
      status('The notes changed since Claude read them. Press Read with Claude to read them again.');
    }
    ctx.on('notes:changed', onNotes);

    (async () => {
      claude = !!(await ctx.call('ai', 'status', {}).catch(() => ({}))).claude;
      const mode = $('#mode');
      if (claude) {
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

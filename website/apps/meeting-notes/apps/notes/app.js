/* notes: the left column. Takes the meeting's notes or transcript (typed, pasted, or from a file),
   remembers them, and publishes them. It does not read them: that is the ai part's job.
   Emits: notes:changed (retained: { title, text, hash }).  Capabilities: storage. */
Kernel.register({
  name: 'notes',
  emits: { 'notes:changed': { retain: true } },
  caps: ['storage'],
  init(ctx) {
    const $ = ctx.$, MAX_FILE = 2 * 1024 * 1024;
    const saved = ctx.store.get('notes') || {};
    $('#title').value = saved.title || '';
    $('#text').value = saved.text || '';

    const say = (text, warn) => { $('#note').textContent = text; $('#note').classList.toggle('warn', !!warn); };
    function publish() {
      const title = $('#title').value.trim(), text = $('#text').value;
      const words = (text.match(/\S+/g) || []).length, lines = text ? text.split('\n').length : 0;
      $('#count').textContent = text.trim() ? Text.plural(words, 'word') + ', ' + Text.plural(lines, 'line') : '';
      ctx.store.set('notes', { title, text });
      ctx.emit('notes:changed', { title, text, hash: Text.hash(title + '\n' + text) });
    }
    let timer = null;
    const soon = () => { clearTimeout(timer); timer = setTimeout(publish, 400); };
    $('#title').addEventListener('input', soon);
    $('#text').addEventListener('input', soon);

    function replace(title, text, what) {
      $('#title').value = title; $('#text').value = text;
      clearTimeout(timer); publish();
      say(what);
    }

    $('#sample').addEventListener('click', () => replace(SAMPLE_NOTES.title, SAMPLE_NOTES.text, 'Loaded the sample meeting.'));
    $('#clear').addEventListener('click', () => { replace('', '', 'Cleared.'); $('#text').focus(); });
    $('#open').addEventListener('click', () => $('#file').click());
    $('#file').addEventListener('change', async () => {
      const f = $('#file').files[0];
      $('#file').value = '';
      if (!f) return;
      if (f.size > MAX_FILE) { say(f.name + ' is larger than 2 MB. Open a shorter file, or paste part of it.', true); return; }
      let text;
      try { text = await f.text(); } catch (e) { say('Could not read ' + f.name + ': ' + e.message, true); return; }
      if (/\u0000/.test(text.slice(0, 4000))) { say(f.name + ' is not a text file. Open a .txt or .md file.', true); return; }
      // The first Markdown heading, or else the file name, becomes the meeting's name.
      const heading = /^#\s+(.+)$/m.exec(text);
      replace(heading ? heading[1].trim().slice(0, 120) : f.name.replace(/\.(txt|md|markdown|text)$/i, ''), text.replace(/\r\n?/g, '\n'), 'Opened ' + f.name + '.');
    });

    // Let the viewer drop a file on the text box, too.
    $('#text').addEventListener('dragover', e => { if (e.dataTransfer.types.includes('Files')) e.preventDefault(); });
    $('#text').addEventListener('drop', e => {
      const f = e.dataTransfer.files[0];
      if (!f) return;
      e.preventDefault();
      const dt = new DataTransfer(); dt.items.add(f);
      $('#file').files = dt.files;
      $('#file').dispatchEvent(new Event('change'));
    });

    publish();
  }
});

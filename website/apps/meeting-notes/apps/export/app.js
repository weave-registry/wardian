/* export: the whole result as Markdown. It downloads it through the host (claude:downloads), copies
   it, and shows it so the viewer can see what they get. It uses the viewer's checklist, with their
   ticks and edits, rather than the reading's raw action items.
   Listens: notes:extracted, actions:changed.  Capabilities: claude:downloads. */
Kernel.register({
  name: 'export',
  listens: ['notes:extracted', 'actions:changed'],
  caps: ['claude:downloads'],
  init(ctx) {
    const $ = ctx.$;
    let reading = null, items = null, downloads = null, md = '';
    const say = (text, warn) => { $('#note').textContent = text; $('#note').classList.toggle('warn', !!warn); };
    // Markdown would read a leading "#", "-" or "1." in someone's text as formatting.
    const esc = s => String(s).replace(/\s+/g, ' ').trim().replace(/^([#>*+-]|\d+\.)\s/, '\\$1 ').replace(/([*_`[\]])/g, '\\$1');

    function build() {
      if (!reading || reading.empty) return '';
      const out = ['# ' + esc(reading.title), ''];
      const when = new Date(reading.at).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' });
      out.push('_' + when + ' · ' + (reading.source === 'claude' ? 'Read by Claude' : 'Read by the quick reader') + (reading.stale ? ' · the notes changed after this reading' : '') + '_', '');
      if (reading.summary) out.push('## Summary', '', reading.summary.trim(), '');
      const list = (head, xs) => { if (xs.length) out.push('## ' + head, '', ...xs.map(x => '- ' + esc(x)), ''); };
      list('Decisions', reading.decisions);
      const todo = (items || []).filter(i => i.task && i.task.trim());
      if (todo.length) {
        out.push('## Action items', '');
        for (const i of todo) {
          const extra = [i.owner && '**' + esc(i.owner) + '**', i.due && 'due ' + esc(i.due)].filter(Boolean).join(', ');
          out.push('- [' + (i.done ? 'x' : ' ') + '] ' + esc(i.task) + (extra ? ' — ' + extra : ''));
        }
        out.push('');
      }
      list('Open questions', reading.open_questions);
      return out.join('\n').replace(/\n+$/, '\n');
    }
    function sync() {
      md = build();
      $('#md').value = md;
      $('#md').hidden = !md;
      $('#copy').disabled = !md;
      $('#save').disabled = !(md && downloads);
      $('#sub').textContent = md ? Text.plural(md.split('\n').length, 'line') : '';
    }

    ctx.cap('downloads').then(d => { downloads = d; if (!d) say('Downloads are not available in this host. Copy the text instead.', true); sync(); },
      () => say('Downloads are not available in this host. Copy the text instead.', true));
    ctx.on('notes:extracted', r => { reading = r; sync(); });
    ctx.on('actions:changed', p => { items = p.items; sync(); });

    $('#save').addEventListener('click', async () => {
      const name = Text.slug(reading.title) + '-' + Text.today() + '.md';
      try { await downloads.save({ filename: name, data: md }); say('Saved ' + name + '.'); }
      catch (e) { say('Could not save: ' + e.message, true); }
    });
    // The frame is sandboxed, so the clipboard API may be refused; then select the text and use the
    // older copy command, and as a last resort ask the viewer to press the keys.
    $('#copy').addEventListener('click', async () => {
      const policy = document.permissionsPolicy || document.featurePolicy;
      if (navigator.clipboard && (!policy || policy.allowsFeature('clipboard-write'))) {
        try { await navigator.clipboard.writeText(md); say('Copied ' + Text.plural(md.split('\n').length, 'line') + '.'); return; } catch { /* try the older way */ }
      }
      const box = $('#md');
      box.focus(); box.select();
      let done = false;
      try { done = document.execCommand('copy'); } catch { done = false; }
      say(done ? 'Copied.' : 'Your browser blocked copying here. The text is selected: press Ctrl+C (⌘C on a Mac).', !done);
    });
    sync();
  },
  // Save as web page: the Markdown as text, which is the useful part of this panel on paper.
  snapshot(ctx) {
    const box = ctx.el('div'), h = ctx.el('h2'), pre = ctx.el('pre');
    h.textContent = 'Export (Markdown)';
    pre.className = 'md';
    pre.textContent = ctx.$('#md').value || 'Nothing to export yet.';
    box.append(h, pre);
    return box;
  }
});

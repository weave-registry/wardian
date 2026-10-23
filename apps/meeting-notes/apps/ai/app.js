/* ai: the app's AI, out of sight (ADR-2610131300). Its skill (skill.md) and its tools (tools.json)
   are data that Wardian's App settings show, edit and improve; this code holds only each tool's run,
   on the notes and the reading it hears. It is the only part with claude:sample.
   Without a Claude provider, ctx.cap('sample') is null: status says so, and the reader panel uses
   the quick reader instead.
   Listens: notes:changed, notes:extracted, actions:changed.
   Provides: status ({ claude }), read (the notes as a reading, or { error }), cancel, chat (Wardian's Ask).
   Capabilities: claude:sample, asset. */
const INLINE_CHARS = 40000;          // notes up to this length go in the request; Claude takes 60,000 characters
const READING = {
  type: 'object',
  properties: {
    summary: { type: 'string' },
    decisions: { type: 'array', maxItems: 12, items: { type: 'string' } },
    action_items: { type: 'array', maxItems: 20, items: { type: 'object', properties: {
      owner: { type: 'string' }, task: { type: 'string', minLength: 1 }, due: { type: 'string' } }, required: ['task'] } },
    open_questions: { type: 'array', maxItems: 10, items: { type: 'string' } },
  },
  required: ['summary', 'decisions', 'action_items', 'open_questions'],
};

Kernel.register({
  name: 'ai',
  listens: ['notes:changed', 'notes:extracted', 'actions:changed'],
  provides: ['status', 'read', 'cancel', 'chat'],
  caps: ['claude:sample', 'asset'],
  init(ctx) {
    let notes = { title: '', text: '', hash: '' }, reading = null, checklist = null, controller = null;
    ctx.on('notes:changed', p => { notes = p; });
    ctx.on('notes:extracted', p => { reading = p && !p.empty ? p : null; });
    ctx.on('actions:changed', p => { checklist = p; });
    const numbered = (n, line) => n + ': ' + line;

    const ai = WardianUI.agentApp(ctx, { answer: READING, maxSteps: 6, run: {
      current_reading: async () => {
        if (!reading) return { none: 'Nothing has read the notes yet.' };
        return { meeting: reading.title, read_by: reading.source === 'claude' ? 'Claude' : 'the quick reader',
          out_of_date: !!reading.stale || reading.hash !== notes.hash, summary: reading.summary, decisions: reading.decisions,
          open_questions: reading.open_questions, action_items: (checklist && Array.isArray(checklist.items) ? checklist.items : reading.action_items)
            .map(a => ({ owner: a.owner || '', task: a.task, due: a.due || '', done: !!a.done })) };
      },
      search_notes: async ({ query, max_results = 20 }) => {
        const q = query.toLowerCase(), lines = notes.text.split('\n'), hits = [];
        for (let i = 0; i < lines.length && hits.length < max_results; i++) {
          if (lines[i].toLowerCase().includes(q)) hits.push(numbered(i + 1, Text.clip(lines[i], 300)));
        }
        return { lines: hits, total_lines: lines.length, more: hits.length === max_results };
      },
      read_notes: async ({ from_line, max_lines = 40 }) => {
        const lines = notes.text.split('\n'), out = [];
        let size = 0;
        for (let i = from_line - 1; i < lines.length && out.length < max_lines && size < 3000; i++) {
          out.push(numbered(i + 1, lines[i]));
          size += lines[i].length + 8;
        }
        return { meeting: notes.title || '(no name)', lines: out, total_lines: lines.length };
      },
    } });

    ctx.provide({
      status: async () => ({ claude: !!(await ctx.cap('sample').catch(() => null)), inline_chars: INLINE_CHARS }),
      /** The notes heard last, read by Claude: { reading, hash } or { error: { code, message } }. */
      async read() {
        const n = notes;
        if (!n.text.trim()) return { error: { code: 'empty', message: 'There are no notes to read.' } };
        if (n.text.length > INLINE_CHARS) return { error: { code: 'prompt_too_large', message: 'The notes are too long for one request.' } };
        if (controller) controller.abort();
        const mine = controller = new AbortController();
        try {
          const task = 'Read these meeting notes and answer with the reading.\nMeeting: ' + (n.title || '(no name)') + '\n<notes>\n' + n.text + '\n</notes>';
          const out = await ai.ask(task, { signal: mine.signal });
          if (out.error) return { error: out.error };
          try { return { reading: Text.reading(out.answer), hash: n.hash }; }
          catch (e) { return { error: { code: 'invalid_json', message: e.message } }; }
        } finally {
          if (controller === mine) controller = null;
        }
      },
      cancel: () => { if (controller) controller.abort(); return true; },
      chat: ({ messages }, call) => ai.chat(messages, call),
    });
  }
});

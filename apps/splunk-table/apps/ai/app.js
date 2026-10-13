/* ai: the app's AI, out of sight (ADR-2610131300). Its skill (skill.md) and tools (tools.json) are data
   that Wardian's App settings show, edit and improve; this code holds only each Splunk tool's run, on
   fixed searches that take only an index and a sourcetype, so Claude never runs a search of its own.
   list_tables and read_rows are the agent component's own, on this app's tables.
   Emits: ai:step (each tool Claude uses while it writes).  Provides: status ({ claude, splunk }),
   write ({ what, usl } → { answer } or { error }), chat (Wardian's Ask).
   Capabilities: claude:sample, asset, splunk, db. */
Kernel.register({
  name: 'ai',
  emits: {'ai:step': {}},
  provides: ['status', 'write', 'chat'],
  caps: ['claude:sample', 'asset', 'splunk', 'db'],
  init(ctx){
    const splunkP = ctx.cap('splunk').catch(() => null);
    const NAME = /^[\w:.\-]{1,80}$/;
    const noEmails = s => String(s).replace(/[\w.+-]+@[\w-]+(\.[\w-]+)+/g, '<email>');
    function where({index, sourcetype}){
      if (!NAME.test(index) || (sourcetype && !NAME.test(sourcetype))) throw new Error('an index or sourcetype is a name: letters, digits, _ : . and - only');
      return 'index="' + index + '"' + (sourcetype ? ' sourcetype="' + sourcetype + '"' : '');
    }
    async function search(spl){
      const s = await splunkP;
      if (!s) throw new Error('Splunk is not set up in this Wardian');
      return s.search({search: spl, earliest: '-24h'});
    }
    const ai = WardianUI.agentApp(ctx, { db: true, maxSteps: 8,
      onStep: s => ctx.emit('ai:step', {step: s.step, tool: s.tool}),
      run: {
        list_data: async () => ({lines: (await search('| tstats count WHERE index=* BY index sourcetype | sort -count | head 60')).rows.map(r => r.join('  '))}),
        field_summary: async i => ({fields: (await search(where(i) + ' | head 5000 | fieldsummary | where count>50 | sort -count | head 40 | table field count distinct_count')).rows.map(r => r.join('  '))}),
        sample_events: async i => ({events: (await search(where(i) + ' | head 2000 | dedup punct | head ' + (i.count || 6) + ' | table _raw')).rows.map(r => noEmails(r[0]).slice(0, 600))}),
      } });

    ctx.provide({
      async status(){
        const sample = await ctx.cap('sample').catch(() => null), s = await splunkP;
        return {claude: !!sample, splunk: !!s && !!(await s.status().catch(() => ({ready: false}))).ready};
      },
      /** A search for what the user wants: { answer: { search, earliest, explanation, … } } or { error: { code, message } }. */
      async write({what, usl}){
        const fields = 'search, earliest, explanation, title, method' + (usl ? ', loadUnit, throughputUnit, responseUnit, load, throughput, response' : '');
        const task = 'Write a Splunk search (SPL) for this request: "' + String(what).slice(0, 500) + '"\n' +
          (usl ? 'The table is for the USL lab: follow "Searches for the USL lab".' : 'The table is a plain table: follow "Searches for a table".') + '\n' +
          'Look at the data first, as the skill says. Then answer with an object of ' + fields + ', as "The answer" describes.';
        const out = await ai.ask(task);
        if (out.error) return {error: out.error};
        const a = out.answer;
        if (!a || typeof a !== 'object' || typeof a.search !== 'string' || !a.search.trim()) return {error: {code: 'bad_reply', message: 'Claude did not return a search.'}};
        return {answer: a};
      },
      chat: ({messages}) => ai.chat(messages),
    });
  }
});

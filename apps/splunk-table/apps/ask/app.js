/* ask: Claude writes the search. It finds the likely index, reads its fields and a few events, writes
   SPL, then hands it to the search part, which runs it. Without Claude or Splunk the panel says which
   is missing and where an admin sets it up, with the box and button turned off.
   Emits: search:use.  Capabilities: splunk, claude:sample. */
Kernel.register({
  name: 'ask',
  emits: {'search:use': {}},
  caps: ['splunk', 'claude:sample'],
  init(ctx){
    const $ = ctx.$, bar = $('#bar');
    function status(text, kind){ const st = $('#status'); st.className = 'status' + (kind ? ' ' + kind : ''); st.textContent = text; }
    const noEmails = s => String(s).replace(/[\w.+-]+@[\w-]+(\.[\w-]+)+/g, '<email>');
    const str = v => typeof v === 'string' ? v.slice(0, 600) : '';
    // Off until Claude and Splunk both answer, so nothing is sent from a half-set-up Wardian.
    const off = why => { $('#aiWhat').disabled = true; $('#btnAi').disabled = true; status(why, 'warn'); };
    $('#aiWhat').disabled = true; $('#btnAi').disabled = true;

    async function write(sample, splunk){
      const what = $('#aiWhat').value.trim(), usl = $('#aiUsl').checked;
      if (!what){ status('Describe what you want first.', 'warn'); $('#aiWhat').focus(); return; }
      $('#btnAi').disabled = true;
      status('');
      try {
        bar.start('Step 1 of 4: listing the data in Splunk', {value: 0, max: 4});
        const inv = await splunk.search({search: '| tstats count WHERE index=* BY index sourcetype | sort -count | head 60', earliest: '-24h'});
        const list = inv.rows.map(r => r.join('  ')).join('\n');
        bar.update({value: 1, label: 'Step 2 of 4: Claude picks where to look'});
        const goal = usl ? 'A user wants to fit the Universal Scalability Law (throughput against load) to data in Splunk.' : 'A user wants a table of data from Splunk.';
        const pick = await sample.json(goal + ' What they want: "' + what.replace(/"/g, "'") + '"\n\n' +
          'Events in the last 24 hours, by index and sourcetype:\n' + list + '\n\n' +
          'Pick the one index and sourcetype most likely to hold this data. Reply as JSON: {"index":"…","sourcetype":"…","why":"one sentence"}', {modelTier: 'quick'});
        if (!pick || !pick.index) throw new Error('Claude did not pick an index.');
        const where = 'index="' + String(pick.index).replace(/"/g, '') + '"' + (pick.sourcetype ? ' sourcetype="' + String(pick.sourcetype).replace(/"/g, '') + '"' : '');
        bar.update({value: 2, label: 'Step 3 of 4: reading the fields of ' + where});
        const fields = await splunk.search({search: where + ' | head 5000 | fieldsummary | where count>50 | sort -count | head 40 | table field count distinct_count', earliest: '-24h'});
        const raws = await splunk.search({search: where + ' | head 2000 | dedup punct | head 6 | table _raw', earliest: '-24h'});
        bar.update({value: 3, label: 'Step 4 of 4: Claude writes the search'});
        const rules = usl ? [
          '- It must start with ' + where + ' and end with "| table <load> <throughput> <response time>": those columns first, in that order, numbers only, one row per load level, load > 0. You may add one more column after them that says how much data each row rests on (for example minutes or count).',
          '- If the data is a load test with a load or concurrency field, group by it.',
          '- If it is production traffic with one event per finished request and a duration field, use Little\'s Law: per 1-minute bucket, throughput x = count/60, response time r = average duration, load n = x * r (in seconds); round n to a step that gives 10 to 30 levels; average x and r per level, and count the minutes per level; keep levels seen in at least 10 minutes.',
          '- Leave out requests that are not normal work, such as long-polls, event streams, health checks or failures, when the fields show them.',
        ] : [
          '- It must start with ' + where + ' and end with "| table <columns>", naming the columns the user wants.',
          '- Keep the result small: aggregate with stats or timechart rather than return raw events, unless the user asks for events.',
        ];
        const out = await sample.json('Write a Splunk search (SPL) for this request.\n\n' +
          'What the user wants: "' + what.replace(/"/g, "'") + '"\nData: ' + where + ' (' + (pick.why || '') + ')\n\n' +
          'Fields (name, events with it, distinct values), from 5000 recent events:\n' + fields.rows.map(r => r.join('  ')).join('\n') + '\n\n' +
          'Sample events:\n' + raws.rows.map(r => noEmails(r[0]).slice(0, 600)).join('\n') + '\n\n' +
          'Rules for the search:\n' + rules.join('\n') + '\n- Use only fields that exist above. Do not invent fields.\n\n' +
          'Reply as JSON: {"search":"…","earliest":"a Splunk time like -7d","explanation":"two sentences for the user: what the search measures and any caveat",' +
          '"title":"a short name for this data"' + (usl ? ',"loadUnit":"…","throughputUnit":"…","responseUnit":"ms or s","load":"one plain sentence: what the load column counts","throughput":"one plain sentence: what the throughput column counts","response":"one plain sentence: what the response time column is, with its unit"' : '') +
          ',"method":"two or three plain sentences: how the search turns events into rows, for someone who does not know Splunk"}');
        if (!out || typeof out.search !== 'string') throw new Error('Claude did not return a search.');
        // The search part puts it in the form and runs it, with the explanation in its status.
        ctx.emit('search:use', {search: out.search.trim(), range: typeof out.earliest === 'string' ? out.earliest : '-7d',
          units: usl ? {n: str(out.loadUnit), x: str(out.throughputUnit), r: out.responseUnit === 's' ? 's' : 'ms'} : null,
          about: {title: str(out.title) || 'Search written by Claude', load: str(out.load), throughput: str(out.throughput), response: str(out.response),
            method: str(out.method) + ' Claude wrote this search from: “' + what.slice(0, 200) + '”.'},
          why: str(out.explanation)});
        bar.done('Search written');
        status('Claude wrote the search. It is running under Search.');
      } catch (e) {
        bar.fail('Could not write the search');
        status('Could not write the search: ' + Cells.errText(e), 'warn');
      } finally { $('#btnAi').disabled = false; }
    }

    (async () => {
      const sample = await ctx.cap('sample').catch(() => null);
      const splunk = await ctx.cap('splunk').catch(() => null);
      const splunkReady = !!splunk && (await splunk.status().catch(() => ({ready: false}))).ready;
      if (!sample && !splunkReady) return off('Claude and Splunk are not set up in this Wardian. An admin sets them up in Settings → Claude and Settings → Splunk.');
      if (!sample) return off('Claude is not set up in this Wardian. An admin sets it up in Settings → Claude: an Anthropic API key, or Amazon Bedrock with an AWS profile.');
      if (!splunkReady) return off('Splunk is not set up in this Wardian. An admin adds it in Settings → Splunk.');
      $('#aiWhat').disabled = false; $('#btnAi').disabled = false;
      $('#btnAi').addEventListener('click', () => write(sample, splunk));
    })();
  }
});

/* ask: Claude writes the search. The out-of-sight ai part looks at the data in Splunk with its tools
   (the likely index, its fields and a few events) and writes the SPL; this panel shows its steps,
   then hands the search to the search part, which runs it. Without Claude or Splunk the panel says
   which is missing and where an admin sets it up, with the box and button turned off.
   Listens: ai:step.  Emits: search:use.  Needs: ai.status, ai.write. */
Kernel.register({
  name: 'ask',
  emits: {'search:use': {}},
  listens: ['ai:step'],
  needs: ['ai.status', 'ai.write'],
  init(ctx){
    const $ = ctx.$, bar = $('#bar');
    function status(text, kind){ const st = $('#status'); st.className = 'status' + (kind ? ' ' + kind : ''); st.textContent = text; }
    const str = v => typeof v === 'string' ? v.slice(0, 600) : '';
    // Off until Claude and Splunk both answer, so nothing is sent from a half-set-up Wardian.
    const off = why => { $('#aiWhat').disabled = true; $('#btnAi').disabled = true; status(why, 'warn'); };
    $('#aiWhat').disabled = true; $('#btnAi').disabled = true;
    const STEP = {list_data: 'listing the data in Splunk', field_summary: 'reading the fields', sample_events: 'reading a few events',
      list_tables: 'looking at the tables kept here', read_rows: 'reading a table'};
    let writing = false;
    ctx.on('ai:step', s => { if (writing) bar.update({label: 'Claude is ' + (STEP[s.tool] || 'working') + ' (step ' + s.step + ')'}); });
    const WORDS = {
      not_granted: 'You did not allow this app to use Claude. Press the button again to be asked again, or change it in Wardian’s Settings → Permissions.',
      unavailable: 'Claude is not set up in this Wardian, or this app may not use it.',
      rate_limited: 'Claude’s rate limit was reached. Wait a minute, then try again.',
      over_budget: 'This app has used its Claude tokens for today. An admin can raise its cap in Settings → Claude.',
      max_steps: 'Claude looked at the data for too long without writing a search. Describe what you want more narrowly.',
      bad_reply: 'Claude did not return a search. Try again; answers vary.',
      bad_repository: 'The AI’s skill and tools could not be read. Check them in ⋯ → AI skill and tools.',
    };

    async function write(){
      const what = $('#aiWhat').value.trim(), usl = $('#aiUsl').checked;
      if (!what){ status('Describe what you want first.', 'warn'); $('#aiWhat').focus(); return; }
      $('#btnAi').disabled = true; writing = true;
      status('');
      try {
        bar.start('Claude is looking at the data in Splunk');
        const r = await ctx.call('ai', 'write', {what, usl});
        if (r.error) throw r.error;
        const out = r.answer;
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
        status('Could not write the search: ' + (WORDS[e && e.code] || Cells.errText(e)), 'warn');
      } finally { writing = false; $('#btnAi').disabled = false; }
    }

    (async () => {
      const s = await ctx.call('ai', 'status', {}).catch(() => ({}));
      if (!s.claude && !s.splunk) return off('Claude and Splunk are not set up in this Wardian. An admin sets them up in Settings → Claude and Settings → Splunk.');
      if (!s.claude) return off('Claude is not set up in this Wardian. An admin sets it up in Settings → Claude: an Anthropic API key, or Amazon Bedrock with an AWS profile.');
      if (!s.splunk) return off('Splunk is not set up in this Wardian. An admin adds it in Settings → Splunk.');
      $('#aiWhat').disabled = false; $('#btnAi').disabled = false;
      $('#btnAi').addEventListener('click', write);
    })();
  }
});

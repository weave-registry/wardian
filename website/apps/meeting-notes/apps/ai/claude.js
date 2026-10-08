/* ClaudeReader: asks Claude, through Wardian's claude:sample, to read the notes, and turns each error
   code the host can give (SPEC.md 6.6) into words a person can act on. */
const ClaudeReader = (() => {
  // Wardian takes prompts up to 60 000 characters; the instructions use about 1 000 of them.
  const MAX_CHARS = 58000;

  function prompt(title, text) {
    return 'You read meeting notes and pull out what matters to the people who were there.\n\n' +
      'Reply with JSON only, in exactly this shape:\n' +
      '{"summary": "3 to 5 plain sentences: what the meeting was about and what came out of it",\n' +
      ' "decisions": ["each decision the meeting made, as one sentence"],\n' +
      ' "action_items": [{"owner": "the person who will do it, or \\"\\" if nobody was named", "task": "what to do, starting with a verb", "due": "when, as the notes say it, or \\"\\""}],\n' +
      ' "open_questions": ["each question the meeting left unanswered"]}\n\n' +
      'Rules: use only what the notes say. Do not invent owners, dates or decisions. A promise such as ' +
      '"I\'ll send it" is an action item for the speaker. Leave a list empty when the notes have none. ' +
      'Keep it short, since the answer has a size limit: at most 12 decisions, 20 action items and 10 questions, ' +
      'one sentence each. Write in the language of the notes.\n\n' +
      'Meeting: ' + (title || '(no name)') + '\n<notes>\n' + text + '\n</notes>';
  }

  const coded = (code, message) => Object.assign(new Error(message), { code });

  /** Resolves to a reading, or rejects with an Error whose code is one of SPEC.md 6.6's. */
  async function read(sample, title, text, signal) {
    if (text.length > MAX_CHARS) throw coded('prompt_too_large', 'too long');
    const out = await sample.json(prompt(title, text), { signal });
    try { return Text.reading(out); }
    catch (e) { throw coded('invalid_json', e.message); }       // JSON, but not a reading
  }

  const WORDS = {
    not_granted: 'Wardian did not get your permission to send these notes to Claude. Press the button again to be asked again. If you chose “Don’t allow”, change it in Wardian’s Settings → Permissions.',
    rate_limited: 'Claude’s rate limit was reached. Wait a minute, then try again.',
    refused: 'Claude declined to read these notes.',
    invalid_json: 'Claude’s answer was not in the expected form. Try again; answers vary.',
    prompt_too_large: 'The notes are too long for one request (over ' + MAX_CHARS.toLocaleString() + ' characters, or over the model’s limit). Shorten them, or split the meeting into parts.',
    cancelled: 'Stopped. Claude may still finish on the server, but this app ignores that answer.',
  };
  /** { code, text } for any error from read(). */
  function explain(e) {
    const code = e && Object.prototype.hasOwnProperty.call(WORDS, e.code) ? e.code : 'error';
    return { code, text: code === 'error' ? 'Could not reach Claude: ' + ((e && e.message) || String(e)).replace(/\.?$/, '.') : WORDS[code] };
  }
  return Object.freeze({ read, explain, MAX_CHARS });
})();

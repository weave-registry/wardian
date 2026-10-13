/* Words: what the reader panel says for each way a read by Claude can end without a reading. The
   codes are claude:sample's (SPEC.md 6.6) and the agent component's, as the ai part returns them. */
const Words = (() => {
  const WORDS = {
    not_granted: 'Wardian did not get your permission to send these notes to Claude. Press the button again to be asked again. If you chose “Don’t allow”, change it in Wardian’s Settings → Permissions.',
    unavailable: 'Claude is not set up in this Wardian, or this app may not use it.',
    rate_limited: 'Claude’s rate limit was reached. Wait a minute, then try again.',
    over_budget: 'This app has used its Claude tokens for today. An admin can raise its cap in Settings → Claude.',
    refused: 'Claude declined to read these notes.',
    invalid_json: 'Claude’s answer was not in the expected form. Try again; answers vary.',
    bad_reply: 'Claude’s answer was not in the expected form. Try again; answers vary.',
    max_steps: 'Claude did not finish the reading. Try again.',
    bad_repository: 'The AI’s skill and tools could not be read. Check them in ⋯ → AI skill and tools.',
    prompt_too_large: 'The notes are too long for one request (over 40,000 characters). Shorten them, or split the meeting into parts.',
    empty: 'Paste or open some notes first.',
    cancelled: 'Stopped. Claude may still finish on the server, but this app ignores that answer.',
  };
  /** { code, text } for an error { code, message } from the ai part. */
  function explain(e) {
    const code = e && Object.prototype.hasOwnProperty.call(WORDS, e.code) ? e.code : 'error';
    return { code, text: code === 'error' ? 'Could not reach Claude: ' + ((e && e.message) || String(e)).replace(/\.?$/, '.') : WORDS[code] };
  }
  return Object.freeze({ explain });
})();

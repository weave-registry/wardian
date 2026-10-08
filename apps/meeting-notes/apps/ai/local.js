/* LocalReader: reads notes without AI, line by line, by the patterns people write in notes. It always
   gives the same answer for the same text. It knows:
     actions    "Action: …", "TODO …", "Next step: …", "- [ ] …", "@name will …", "Name to …",
                and a speaker who says "I'll …" / "I will …" in a transcript ("Marco: I'll resubmit …")
     owners     "@name" anywhere on the line, "Name: …" after a check box, the speaker of an "I'll"
     due dates  "by Friday", "due Oct 22", "before 2026-11-01", "tomorrow", "end of week" …
     decisions  "Decision: …", "Decided: …", "Agreed: …", "We decided to …", "Agreed to …"
     questions  "Q: …", "Question: …", "Open question: …", and any line that ends with "?" */
const LocalReader = (() => {
  const DAY = '(?:(?:mon|tues|wednes|thurs|fri|satur|sun)day|mon|tue|wed|thu|thur|fri|sat|sun)\\b';
  const MONTH = '(?:jan|feb|mar|apr|may|jun|jul|aug|sep|sept|oct|nov|dec)[a-z]*\\.?';
  const WHEN = '(' + [DAY, 'tomorrow(?: morning| afternoon)?', 'today', 'tonight', 'end of (?:the )?(?:day|week|month|sprint)',
    'eod', 'eow', 'next ' + DAY, 'next week', MONTH + ' \\d{1,2}(?:st|nd|rd|th)?', '\\d{1,2}(?:st|nd|rd|th)? (?:of )?' + MONTH,
    '\\d{4}-\\d{2}-\\d{2}', '\\d{1,2}/\\d{1,2}(?:/\\d{2,4})?', 'the \\d{1,2}(?:st|nd|rd|th)'].join('|') + ')';
  const DUE = new RegExp('[,;]?\\s*\\(?\\b(?:by|due|before|until)\\b\\s*:?\\s*' + WHEN + '\\)?', 'i');
  const BARE = /[,;]?\s*\b(tomorrow(?: morning| afternoon)?|end of (?:the )?(?:day|week))\b/i;

  const ACTION = /^(?:action(?:\s*items?)?|todo|to-do|to do|next steps?|follow[- ]up)\b\s*[:\-–]?\s*(.+)$/i;
  const DECISION = /^(?:decision|decided|agreed|resolved|conclusion)\s*[:\-–]\s*(.+)$/i;
  const DECIDED = /^(?:we\s+)?(?:have\s+)?(?:decided|agreed)\s+(?:to|that|on)\b/i;
  const QUESTION = /^(?:q|question|open question|open)\s*[:\-–]\s*(.+)$/i;
  const SPEAKER = /^([A-Z][\w.'-]*(?: [A-Z][\w.'-]*)?)\s*:\s+(.+)$/;          // "Marco: …", "Ana Ruiz: …"
  const PROMISE = /\b(?:I'll|I will|I'm going to|I am going to)\s+(.+)$/i;
  const AT = /\(?@([A-Za-z][\w.-]*)\)?/;

  const cap = s => s ? s[0].toUpperCase() + s.slice(1) : s;
  const tidy = s => cap(s.replace(/\s+/g, ' ').replace(/^[\s,;:\-–]+|[\s,;:\-–]+$/g, '').replace(/\.$/, ''));

  /** Takes the owner and due date out of an action's text. */
  function item(task, owner = '') {
    let due = '';
    const d = DUE.exec(task) || BARE.exec(task);
    if (d) { due = d[1]; task = task.replace(d[0], ''); }
    const at = AT.exec(task);
    if (at) { owner = owner || at[1]; task = task.replace(at[0], ''); }
    const named = /^([A-Z][a-z]+)\s*(?::|\s+(?:to|will|should|needs to|must))\s+(.+)$/.exec(task.trim());
    if (named && !owner) { owner = named[1]; task = named[2]; }
    return { owner: cap(owner), task: tidy(task), due };
  }

  function read(title, text) {
    const decisions = [], actions = [], questions = [], headings = [];
    let attendees = '', lines = 0;
    for (const raw of text.split('\n')) {
      let line = raw.trim();
      if (!line) continue;
      lines++;
      const h = /^#{1,6}\s+(.+)$/.exec(line);
      if (h) { headings.push(h[1].trim()); continue; }
      const box = /^[-*•]?\s*\[( |x|X)?\]\s*(.+)$/.exec(line);
      line = line.replace(/^(?:[-*•]|\d+[.)])\s+/, '');
      if (/^(attendees|present|participants)\s*:/i.test(line)) { attendees = line; continue; }
      let m;
      if (box) { actions.push({ ...item(box[2]), done: /x/i.test(box[1] || '') }); continue; }
      if ((m = ACTION.exec(line)) && !/^next steps?$/i.test(line)) { actions.push(item(m[1])); continue; }
      if ((m = DECISION.exec(line))) { decisions.push(tidy(m[1]) + '.'); continue; }
      if (DECIDED.test(line)) { decisions.push(tidy(line) + '.'); continue; }
      if ((m = QUESTION.exec(line))) { questions.push(tidy(m[1].replace(/\?+$/, '')) + '?'); continue; }
      if ((m = /^@([A-Za-z][\w.-]*)\s+(?:will|to|should|needs to|must)\s+(.+)$/i.exec(line))) { actions.push(item(m[2], m[1])); continue; }
      const sp = SPEAKER.exec(line), said = sp ? sp[2] : line;
      if (sp && (m = PROMISE.exec(said))) { actions.push(item(m[1], sp[1])); continue; }
      if (/\?\s*$/.test(said)) { questions.push(tidy(said) + (sp ? ' (' + sp[1] + ')' : '')); continue; }
      if (AT.test(line) && /^(?:please\s+)?[a-z]+\s/i.test(line) && /\b(will|to|please|todo)\b/i.test(line)) actions.push(item(line));
    }
    const topics = headings.filter(h => h.toLowerCase() !== String(title).trim().toLowerCase());
    const parts = [];
    if (attendees) parts.push(tidy(attendees) + '.');
    if (topics.length) parts.push('The notes cover ' + (topics.length > 1 ? topics.slice(0, -1).join(', ') + ' and ' + topics[topics.length - 1] : topics[0]) + '.');
    parts.push('The quick reader found ' + Text.plural(decisions.length, 'decision') + ', ' + Text.plural(actions.length, 'action item') +
      ' and ' + Text.plural(questions.length, 'open question') + ' in ' + Text.plural(lines, 'line') + '.');
    return Text.reading({ summary: parts.join(' '), decisions, action_items: actions, open_questions: questions });
  }
  return Object.freeze({ read });
})();

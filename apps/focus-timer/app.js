/* Focus timer: focus, short break, long break, in turn. When a focus session ends, or is stopped
   after at least a minute, it goes out on the channel "focus.session" as
     {label, minutes, started, ended, completed}
   and the Focus log keeps it. Wardian asks the user before the first send.
   The clock counts against the wall clock (Date.now()), not against timer ticks, so a tab the
   browser slows down in the background still ends each phase on time. */
(() => {
  'use strict';
  const $ = id => document.getElementById(id);
  const CHANNEL = 'focus.session', MIN_LOG_MS = 60000;
  const NAMES = {work: 'Focus', short: 'Short break', long: 'Long break'};
  const toast = (text, opts) => window.WardianUI && WardianUI.toast ? WardianUI.toast(text, opts) : null;

  const S = {
    phase: 'work', running: false, endsAt: 0, left: 0, round: 0,   // round: focus sessions done this cycle
    started: null, worked: 0, resumed: 0,                          // the current focus session
    fresh: true,                                                   // the phase has not started yet
  };
  const minutesOf = id => Math.max(1, Math.min(180, Math.round(Number($(id).value)) || 1));
  const length = phase => minutesOf(phase) * 60000;
  S.left = length('work');

  // ---------- sound: a soft two-note chime, made with WebAudio ----------
  let audio = null;
  function chime(){
    if (!$('sound').checked) return;
    try {
      audio = audio || new (window.AudioContext || window.webkitAudioContext)();
      const t = audio.currentTime;
      [[660, 0], [880, 0.22]].forEach(([hz, at]) => {
        const o = audio.createOscillator(), g = audio.createGain();
        o.type = 'sine'; o.frequency.value = hz;
        g.gain.setValueAtTime(0.0001, t + at);
        g.gain.exponentialRampToValueAtTime(0.18, t + at + 0.03);
        g.gain.exponentialRampToValueAtTime(0.0001, t + at + 1.2);
        o.connect(g).connect(audio.destination); o.start(t + at); o.stop(t + at + 1.3);
      });
    } catch { /* no sound here: the timer still works */ }
  }

  // ---------- drawing ----------
  const clock = ms => { const s = Math.ceil(ms / 1000); return String(Math.floor(s / 60)).padStart(2, '0') + ':' + String(s % 60).padStart(2, '0'); };
  const label = () => $('label').value.trim();
  function draw(){
    const left = S.running ? Math.max(0, S.endsAt - Date.now()) : S.left, total = length(S.phase);
    $('time').textContent = clock(left);
    const arc = $('arc'), c = 2 * Math.PI * 88;
    arc.style.strokeDasharray = c; arc.style.strokeDashoffset = c * (1 - (total - left) / total);
    document.querySelectorAll('.phase').forEach(p => p.classList.toggle('on', p.dataset.phase === S.phase));
    $('timerCard').dataset.phase = S.phase;
    const fresh = S.fresh;
    $('what').textContent = S.phase === 'work'
      ? (S.running ? (label() || 'Focusing') : fresh ? 'Ready to focus' : 'Paused')
      : (S.running ? NAMES[S.phase] + ': rest' : fresh ? NAMES[S.phase] + ' next' : 'Break paused');
    $('go').textContent = S.running ? 'Pause' : fresh ? 'Start' : 'Resume';
    $('skip').textContent = S.phase === 'work' ? 'Skip to break' : 'Skip break';
    const every = Number($('every').value);
    $('rounds').replaceChildren(...Array.from({length: every}, (_, i) => {
      const d = document.createElement('span'); d.className = 'dot' + (i < S.round ? ' done' : ''); return d;
    }));
    $('rounds').title = S.round + ' of ' + every + ' focus sessions before the long break';
  }

  // ---------- the clock ----------
  function start(){
    if (S.running) return;
    S.running = true; S.fresh = false; S.endsAt = Date.now() + S.left; S.resumed = Date.now();
    if (S.phase === 'work' && !S.started) S.started = new Date().toISOString();
    if (audio && audio.state === 'suspended') audio.resume();
    draw();
  }
  function pause(){
    if (!S.running) return;
    S.left = Math.max(0, S.endsAt - Date.now()); S.running = false;
    if (S.phase === 'work') S.worked += Date.now() - S.resumed;
    draw();
  }
  // Ends the focus session in progress: logs it if it lasted a minute or more.
  function endWork(completed){
    if (S.running) S.worked += Date.now() - S.resumed;
    const ms = S.worked, started = S.started;
    S.started = null; S.worked = 0;
    if (!started) return;
    if (ms < MIN_LOG_MS){ if (!completed) toast('Less than a minute of focus, so it was not logged.'); return; }
    log({label: label() || 'No label', minutes: Math.round(ms / 6000) / 10, started, ended: new Date().toISOString(), completed});
  }
  function next(completed){
    const was = S.phase;
    if (was === 'work'){
      endWork(completed);
      S.round += 1;
      S.phase = S.round >= Number($('every').value) ? 'long' : 'short';
    } else {
      if (was === 'long') S.round = 0;
      S.phase = 'work';
    }
    S.running = false; S.fresh = true; S.left = length(S.phase);
    if (completed){ chime(); toast(was === 'work' ? 'Focus session done. Time for a ' + NAMES[S.phase].toLowerCase() + '.' : 'Break over. Ready to focus again.'); }
    if (completed && $('auto').checked) start();
    draw();
  }
  function reset(){
    if (S.phase === 'work') endWork(false);
    S.running = false; S.fresh = true; S.phase = 'work'; S.left = length('work');
    draw();
  }
  setInterval(() => { if (S.running && Date.now() >= S.endsAt) next(true); else if (S.running) draw(); }, 250);

  // ---------- sending to the Focus log ----------
  const sent = [];
  function why(e){
    const m = String((e && e.message) || e || '');
    if (!window.wardian || /inside Wardian/i.test(m)) return 'This page is open on its own, outside Wardian. Open Focus timer from Wardian\'s app list to send sessions to the Focus log.';
    if (/not allowed/i.test(m)) return 'Wardian did not allow this timer to send sessions. Press Send again and choose Allow. If you chose Don\'t allow, first open Settings → App permissions in Wardian and press Revoke next to focus-timer.';
    if (/did not answer/i.test(m)) return 'Wardian did not answer in time. Press Send again.';
    return 'Not sent: ' + m;
  }
  function send(entry){
    entry.state = 'sending'; entry.why = ''; drawSent();
    const go = window.wardian ? window.wardian.channel(CHANNEL).send(entry.rec) : Promise.reject(new Error('outside'));
    go.then(() => { entry.state = 'sent'; $('channelNote').textContent = ''; },
      e => { entry.state = 'failed'; entry.why = why(e); $('channelNote').textContent = entry.why; toast('The session was not sent to the Focus log.', {variant: 'destructive'}); })
      .finally(drawSent);
  }
  function log(rec){
    const entry = {rec, state: 'sending', why: ''};
    sent.unshift(entry);
    rememberLabel(rec.label);
    send(entry);
  }
  function drawSent(){
    const list = $('sent');
    if (!sent.length) return;
    list.replaceChildren(...sent.map(x => {
      const li = document.createElement('li'), what = document.createElement('div'), b = document.createElement('span');
      const t = new Date(x.rec.ended).toLocaleTimeString(undefined, {hour: 'numeric', minute: '2-digit'});
      what.className = 'what';
      what.append(Object.assign(document.createElement('b'), {textContent: x.rec.label}),
        Object.assign(document.createElement('small'), {textContent: x.rec.minutes + ' min · ' + (x.rec.completed ? 'completed' : 'stopped early') + ' · ended ' + t}));
      b.className = 'w-badge';
      b.dataset.variant = x.state === 'sent' ? 'success' : x.state === 'failed' ? 'destructive' : 'outline';
      b.textContent = x.state === 'sent' ? 'Sent' : x.state === 'failed' ? 'Not sent' : 'Sending…';
      if (x.why) b.title = x.why;
      li.append(what, b);
      if (x.state === 'failed'){
        const again = Object.assign(document.createElement('button'), {type: 'button', className: 'w-button', textContent: 'Send again'});
        again.dataset.variant = 'outline'; again.dataset.size = 'sm';
        again.addEventListener('click', () => send(x));
        li.append(again);
      }
      return li;
    }));
    const ok = sent.filter(x => x.state === 'sent');
    $('tally').textContent = ok.length + ' sent · ' + Math.round(ok.reduce((s, x) => s + x.rec.minutes, 0)) + ' min';
  }
  const labels = new Set();
  function rememberLabel(l){
    if (!l || labels.has(l)) return;
    labels.add(l);
    $('recent').append(Object.assign(document.createElement('option'), {value: l}));
  }

  // ---------- controls ----------
  $('go').addEventListener('click', () => { if (S.running) pause(); else start(); });
  $('skip').addEventListener('click', () => next(false));
  $('reset').addEventListener('click', reset);
  $('test').addEventListener('click', () => {
    const now = Date.now(), mins = minutesOf('work');
    log({label: label() || 'Test session', minutes: mins, started: new Date(now - mins * 60000).toISOString(), ended: new Date(now).toISOString(), completed: true});
  });
  // A new length shows at once when the phase it belongs to has not started yet.
  ['work', 'short', 'long', 'every'].forEach(id => $(id).addEventListener('change', () => {
    if (S.fresh && id === S.phase) S.left = length(S.phase);
    draw();
  }));
  $('label').addEventListener('input', draw);
  document.addEventListener('keydown', e => {
    if (e.code !== 'Space' || /INPUT|SELECT|TEXTAREA|BUTTON/.test(document.activeElement.tagName)) return;
    e.preventDefault(); $('go').click();
  });
  if (!window.wardian) $('channelNote').textContent = 'This page is open outside Wardian, so sessions cannot reach the Focus log. The timer still works.';
  draw();
})();

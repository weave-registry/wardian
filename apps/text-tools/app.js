// Text tools: the page. app.wasm does the text work (see src/lib.rs); this file moves text in and
// out of its memory and draws the answers. Nothing leaves the browser.
const { instance } = await WebAssembly.instantiateStreaming(fetch(new URL('./app.wasm', import.meta.url)));
const wasm = instance.exports;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

// Copies bytes into the module once, runs work(ptr, len), and frees the space again.
// WebAssembly functions take only numbers, so text goes in as an address and a length.
function withBytes(bytes, work) {
  const ptr = wasm.alloc(bytes.length);
  try {
    new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    return work(ptr, bytes.length);
  } finally {
    wasm.dealloc(ptr, bytes.length);
  }
}

// Reads the module's last answer: n bytes at out_ptr(). Read memory.buffer again after every
// call, because a call can grow memory and replace the buffer.
const answer = (n) => decoder.decode(new Uint8Array(wasm.memory.buffer, wasm.out_ptr(), n));

const $ = (id) => document.getElementById(id);
const text = $('text');
const nf = new Intl.NumberFormat();
const toast = (msg, opts) => (window.WardianUI?.toast ? WardianUI.toast(msg, opts) : console.log(msg));

const SAMPLE = `Call me Ishmael. Some years ago—never mind how long precisely—having little or no money in my purse, and nothing particular to interest me on shore, I thought I would sail about a little and see the watery part of the world. It is a way I have of driving off the spleen and regulating the circulation.

Whenever I find myself growing grim about the mouth; whenever it is a damp, drizzly November in my soul; whenever I find myself involuntarily pausing before coffin warehouses, and bringing up the rear of every funeral I meet; and especially whenever my hypos get such an upper hand of me, that it requires a strong moral principle to prevent me from deliberately stepping into the street, and methodically knocking people's hats off—then, I account it high time to get to sea as soon as I can.

— Herman Melville, Moby-Dick (1851)
`;

// ---------------------------------------------------------------- the file, if one is open

// While the text is exactly what the file held, the hash is of the file's own bytes, so it
// matches `shasum -a 256 file` even when the file has \r\n line ends, which a text box drops.
let file = null; // { name, size, bytes, text }
const fromFile = () => file && text.value === file.text;

function showSource() {
  const src = $('source');
  if (fromFile()) src.textContent = `${file.name} · ${size(file.size)}`;
  else if (file) src.textContent = `${file.name} · edited`;
  else src.textContent = 'Typed text';
}

function size(n) {
  if (n < 1024) return `${n} bytes`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

const MAX_FILE = 25 * 1024 * 1024;
async function open(f) {
  if (!f) return;
  if (f.size > MAX_FILE) {
    toast(`${f.name} is ${size(f.size)}. Text tools opens files up to ${size(MAX_FILE)}.`, { variant: 'destructive', title: 'Too big' });
    return;
  }
  const bytes = new Uint8Array(await f.arrayBuffer());
  const decoded = decoder.decode(bytes);
  const bad = (decoded.match(/�/g) || []).length;
  if (bad > 16 && bad > decoded.length / 100) {
    toast(`${f.name} does not look like UTF-8 text. Some characters show as �.`, { title: 'Not plain text?' });
  }
  text.value = decoded;
  file = { name: f.name, size: f.size, bytes, text: text.value };
  update(true);
}

// ---------------------------------------------------------------- drawing the answers

function duration(sec) {
  if (sec === 0) return '0 sec';
  if (sec < 60) return `${sec} sec`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${min} min`;
  return `${Math.floor(min / 60)} h ${min % 60} min`;
}

function drawCounts(s) {
  const tiles = [
    ['Words', nf.format(s.words), 'big'],
    ['Characters', nf.format(s.chars), 'big'],
    ['Without spaces', nf.format(s.chars_no_spaces)],
    ['Sentences', nf.format(s.sentences)],
    ['Paragraphs', nf.format(s.paragraphs)],
    ['Lines', nf.format(s.lines)],
    ['Unique words', nf.format(s.unique_words)],
    ['Avg word length', s.words ? `${s.avg_word_length} letters` : '—'],
    ['Reading time', duration(s.reading_seconds)],
    ['Speaking time', duration(s.speaking_seconds)],
    ['Size (UTF-8)', size(s.bytes)],
    ['Longest word', s.longest_word || '—', 'wide'],
  ];
  $('tiles').replaceChildren(...tiles.map(([label, value, kind]) => {
    const div = document.createElement('div');
    div.className = `tile ${kind || ''}`;
    const dt = document.createElement('dt'); dt.textContent = label;
    const dd = document.createElement('dd'); dd.textContent = value;
    div.append(dt, dd);
    return div;
  }));
}

function drawWords({ total, rows }) {
  const top = rows[0]?.[1] || 1;
  $('wordRows').replaceChildren(...rows.map(([word, n], i) => {
    const tr = document.createElement('tr');
    const cell = (cls, content) => { const td = document.createElement('td'); td.className = cls; td.append(content); tr.append(td); };
    const bar = document.createElement('span');
    bar.style.width = `${(n / top) * 100}%`;
    bar.title = `${((n / total) * 100).toFixed(1)}% of the words counted`;
    cell('num muted', String(i + 1));
    cell('word', word);
    cell('num', nf.format(n));
    cell('bar', bar);
    return tr;
  }));
  $('wordsCaption').textContent = rows.length
    ? `${nf.format(total)} words counted${$('skip').checked ? ', common English words left out' : ''}. Bars are relative to the top word.`
    : 'No words yet.';
}

const CASES = ['upper', 'lower', 'title', 'sentence', 'slug'];
let caseMode = 0;
const caseOutputs = [...document.querySelectorAll('#caseTabs output')];

// ---------------------------------------------------------------- one pass over the text

function update(fileJustOpened = false) {
  const t0 = performance.now();
  const bytes = encoder.encode(text.value);
  withBytes(bytes, (ptr, len) => {
    drawCounts(JSON.parse(answer(wasm.stats(ptr, len))));
    drawWords(JSON.parse(answer(wasm.top_words(ptr, len, Number($('limit').value), $('skip').checked ? 1 : 0))));
    caseOutputs[caseMode].textContent = answer(wasm.change_case(ptr, len, caseMode));
    if (!fromFile()) $('hash').textContent = answer(wasm.sha256(ptr, len));
  });
  if (fromFile()) {
    $('hash').textContent = withBytes(file.bytes, (ptr, len) => answer(wasm.sha256(ptr, len)));
    $('hashOf').textContent = 'of the file';
    $('hashHint').textContent = `The bytes of ${file.name}, as read from disk. It matches \`shasum -a 256 ${file.name}\`.`;
  } else {
    $('hashOf').textContent = 'of the text';
    $('hashHint').textContent = 'The text as UTF-8 bytes, with \\n line ends. Change one character and the whole hash changes.';
  }
  showSource();
  const ms = performance.now() - t0;
  $('timing').textContent = `${size(bytes.length)} in ${ms < 1 ? '<1' : Math.round(ms)} ms`;
  if (fileJustOpened) toast(`${file.name}: ${$('tiles').querySelector('dd').textContent} words.`, { title: 'Opened', variant: 'success', ms: 3000 });
}

// Short text updates on every key; long text waits until typing pauses.
let pending = 0;
function schedule() {
  clearTimeout(pending);
  if (text.value.length < 200_000) requestAnimationFrame(() => update());
  else pending = setTimeout(update, 250);
}

// ---------------------------------------------------------------- copying and saving

async function copy(value, what) {
  try {
    await navigator.clipboard.writeText(value);
  } catch {
    // A sandboxed page may not get the clipboard API; the older command still works on a selection.
    const tmp = Object.assign(document.createElement('textarea'), { value });
    tmp.style.cssText = 'position:fixed;opacity:0';
    document.body.append(tmp);
    tmp.select();
    const ok = document.execCommand('copy');
    tmp.remove();
    if (!ok) { toast('This browser blocked copying. Select the text and press Ctrl+C or ⌘C.', { variant: 'destructive' }); return; }
  }
  toast(`${what} copied.`, { variant: 'success', ms: 2000 });
}

function download(name, value) {
  const url = URL.createObjectURL(new Blob([value], { type: 'text/plain;charset=utf-8' }));
  const a = Object.assign(document.createElement('a'), { href: url, download: name });
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

const baseName = () => (file ? file.name.replace(/\.[^.]+$/, '') : 'text');

// ---------------------------------------------------------------- wiring

text.addEventListener('input', schedule);
$('limit').addEventListener('change', () => update());
$('skip').addEventListener('change', () => update());
$('caseTabs').addEventListener('w-tab-change', (e) => { caseMode = e.detail.index; update(); });

$('open').addEventListener('click', () => $('file').click());
$('file').addEventListener('change', (e) => { open(e.target.files[0]); e.target.value = ''; });
$('sample').addEventListener('click', () => { file = null; text.value = SAMPLE; update(); });
$('clear').addEventListener('click', () => { file = null; text.value = ''; update(); text.focus(); });

const drop = $('drop');
drop.addEventListener('dragover', (e) => { e.preventDefault(); drop.classList.add('over'); });
drop.addEventListener('dragleave', () => drop.classList.remove('over'));
drop.addEventListener('drop', (e) => {
  e.preventDefault();
  drop.classList.remove('over');
  open(e.dataTransfer.files[0]);
});

$('copyCase').addEventListener('click', () => copy(caseOutputs[caseMode].textContent, 'Text'));
$('saveCase').addEventListener('click', () => download(`${baseName()}-${CASES[caseMode]}.txt`, caseOutputs[caseMode].textContent));
$('copyHash').addEventListener('click', () => copy($('hash').textContent, 'Hash'));

text.value = SAMPLE;
update();

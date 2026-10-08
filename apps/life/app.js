// Game of Life: the world lives in app.wasm's memory (see src/lib.rs). This page asks the module to
// step it, then to paint one RGBA pixel per cell into a buffer, and scales that picture onto the
// canvas. Drawing with the mouse sets single cells through set_cell.
const { instance } = await WebAssembly.instantiateStreaming(fetch(new URL('./app.wasm', import.meta.url)));
const wasm = instance.exports;

const $ = id => document.getElementById(id);
const canvas = $('board'), stage = $('stage'), view = canvas.getContext('2d');
const grid = document.createElement('canvas');            // one pixel per cell, scaled up onto the board
const SPEEDS = [1, 2, 4, 8, 15, 30, 60, 120, 240, 500, 1000];   // generations per second
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');

// Patterns in RLE, the format Life programs share: b = dead, o = alive, $ = next row, a number repeats.
const PATTERNS = {
  glider: 'bo$2bo$3o!',
  lwss: 'bo2bo$o4b$o3bo$4o!',
  gosper: '24bo$22bobo$12b2o6b2o12b2o$11bo3bo4b2o12b2o$2o8bo5bo3b2o$2o8bo3bob2o4bobo$10bo5bo7bo$11bo3bo$12b2o!',
  pulsar: '2b3o3b3o2b2$o4bobo4bo$o4bobo4bo$o4bobo4bo$2b3o3b3o2b2$2b3o3b3o2b$o4bobo4bo$o4bobo4bo$o4bobo4bo2$2b3o3b3o!',
  rpent: 'b2o$2ob$bo!',
  acorn: 'bo5b$3bo3b$2o2b3o!',
  replicator: '2b3o$bo2bo$o3bo$o2bo$3o!',
};
const RULE_HINTS = {
  'B3/S23': 'Born with 3 neighbours, survives with 2 or 3. The classic.',
  'B36/S23': 'Like Conway’s, but 6 neighbours also give birth. The replicator copies itself.',
  'B2/S': 'Born with 2 neighbours, never survives. Explodes from almost anything.',
};

let W = 0, H = 0;
let generation = 0, population = 0;
let pixels = { ptr: 0, len: 0 };       // the RGBA buffer inside wasm memory, reused between frames
let colours = {};
let playing = false, frame = 0, last = 0, owed = 0;

function rle(text) {
  const cells = []; let x = 0, y = 0, run = '';
  for (const ch of text) {
    if (ch >= '0' && ch <= '9') { run += ch; continue; }
    const n = run ? Number(run) : 1; run = '';
    if (ch === 'b') x += n;
    else if (ch === 'o') { for (let i = 0; i < n; i++) cells.push([x + i, y]); x += n; }
    else if (ch === '$') { y += n; x = 0; }
    else if (ch === '!') break;
  }
  return cells;
}

// ---- The world ----

function resize(w, h) {
  W = w; H = h;
  wasm.init(W, H);
  grid.width = W; grid.height = H;
  if (pixels.len) wasm.dealloc(pixels.ptr, pixels.len);
  pixels = { ptr: wasm.alloc(W * H * 4), len: W * H * 4 };
  stage.style.aspectRatio = `${W} / ${H}`;
  stage.style.width = `min(100%, calc(76vh * ${W / H}))`;
  $('sizeInfo').textContent = `${W} × ${H} cells, edges wrap around`;
  restart(0);
}

function restart(pop) { generation = 0; population = pop; draw(); }

function setRule(text) {
  const [, b, s] = text.match(/^B(\d*)\/S(\d*)$/);
  const mask = digits => [...digits].reduce((m, d) => m | (1 << Number(d)), 0);
  wasm.set_rule(mask(b), mask(s));
  $('ruleHint').textContent = RULE_HINTS[text] || '';
}

function place(name, x0, y0) {
  const cells = rle(PATTERNS[name]);
  const pw = Math.max(...cells.map(c => c[0])) + 1, ph = Math.max(...cells.map(c => c[1])) + 1;
  if (x0 === undefined) { x0 = Math.floor((W - pw) / 2); y0 = Math.floor((H - ph) / 2); }
  for (const [x, y] of cells) wasm.set_cell(x0 + x, y0 + y, 1);
  population = wasm.population(); draw();
}

function advance(n) {
  population = wasm.step(n);
  generation += n;
  draw();
}

// ---- Drawing ----

function readColours() {
  const probe = document.createElement('canvas').getContext('2d');
  const css = getComputedStyle(document.documentElement);
  const hex = name => { probe.fillStyle = '#000'; probe.fillStyle = css.getPropertyValue(name).trim(); return parseInt(probe.fillStyle.slice(1), 16) || 0; };
  colours = { live: hex('--cell'), dead: hex('--board'), glow: hex('--glow'), line: css.getPropertyValue('--gridline').trim() };
}

function draw() {
  const ratio = Math.min(devicePixelRatio || 1, 2);
  const cw = Math.max(1, Math.round(canvas.clientWidth * ratio)), ch = Math.max(1, Math.round(canvas.clientHeight * ratio));
  if (canvas.width !== cw || canvas.height !== ch) { canvas.width = cw; canvas.height = ch; }
  wasm.render(pixels.ptr, colours.live, colours.dead, colours.glow, $('trail').checked ? 1 : 0);
  // Read memory.buffer only now: a call can grow memory and replace the buffer.
  grid.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(wasm.memory.buffer, pixels.ptr, pixels.len), W, H), 0, 0);
  view.imageSmoothingEnabled = false;
  view.drawImage(grid, 0, 0, cw, ch);
  const cell = cw / W;
  if (cell >= 7) {                                           // grid lines only where cells are big enough
    view.strokeStyle = colours.line; view.lineWidth = 1; view.beginPath();
    for (let x = 1; x < W; x++) { const p = Math.round(x * cell) + 0.5; view.moveTo(p, 0); view.lineTo(p, ch); }
    for (let y = 1; y < H; y++) { const p = Math.round(y * ch / H) + 0.5; view.moveTo(0, p); view.lineTo(cw, p); }
    view.stroke();
  }
  $('gen').textContent = generation.toLocaleString();
  $('pop').textContent = population.toLocaleString();
}

// ---- Playing ----

const rate = () => SPEEDS[Number($('speed').value)];
function tick(t) {
  const dt = Math.min(250, t - last); last = t;
  owed += dt / 1000 * rate();
  const n = Math.floor(owed);
  if (n > 0) { owed -= n; advance(n); }      // fast speeds take many generations in one call
  frame = requestAnimationFrame(tick);
}
function setPlaying(on) {
  playing = on;
  $('play').textContent = on ? 'Pause' : 'Play';
  cancelAnimationFrame(frame);
  if (on) { last = performance.now(); owed = 0; frame = requestAnimationFrame(tick); }
}

// ---- Drawing cells with the mouse or a finger ----

let painting = null;   // { alive, x, y } while the pointer is down
function cellAt(e) {
  const r = canvas.getBoundingClientRect();
  return [Math.floor((e.clientX - r.left) / r.width * W), Math.floor((e.clientY - r.top) / r.height * H)];
}
function paintTo(x, y) {
  // Fill every cell on the line from the last point, so a fast drag leaves no gaps.
  const steps = Math.max(Math.abs(x - painting.x), Math.abs(y - painting.y), 1);
  for (let i = 1; i <= steps; i++) {
    wasm.set_cell(Math.round(painting.x + (x - painting.x) * i / steps), Math.round(painting.y + (y - painting.y) * i / steps), painting.alive);
  }
  painting.x = x; painting.y = y;
  population = wasm.population(); draw();
}
canvas.addEventListener('pointerdown', e => {
  canvas.setPointerCapture(e.pointerId);
  const [x, y] = cellAt(e);
  painting = { alive: wasm.get_cell(x, y) ? 0 : 1, x, y };
  wasm.set_cell(x, y, painting.alive);
  population = wasm.population(); draw();
});
canvas.addEventListener('pointermove', e => { if (painting) paintTo(...cellAt(e)); });
canvas.addEventListener('pointerup', () => { painting = null; });
canvas.addEventListener('pointercancel', () => { painting = null; });

// ---- Wiring ----

const showSpeed = () => { $('speedOut').textContent = `${rate()} / s`; };
const randomFill = () => restart(wasm.randomize(0.28, (Math.random() * 2 ** 32) >>> 0 || 1));
const clearAll = () => { setPlaying(false); wasm.clear(); restart(0); };
$('play').addEventListener('click', () => setPlaying(!playing));
$('step').addEventListener('click', () => { setPlaying(false); advance(1); });
$('skip').addEventListener('click', () => advance(100));
$('speed').addEventListener('input', showSpeed);
$('random').addEventListener('click', randomFill);
$('clear').addEventListener('click', clearAll);
$('place').addEventListener('click', () => place($('pattern').value));
$('rule').addEventListener('change', e => setRule(e.target.value));
$('size').addEventListener('change', e => { const [w, h] = e.target.value.split('x').map(Number); resize(w, h); });
$('trail').addEventListener('change', draw);
addEventListener('keydown', e => {
  if (e.target.closest('button, input, select, textarea') || e.ctrlKey || e.metaKey || e.altKey) return;
  const k = e.key.toLowerCase();
  if (k === ' ') { e.preventDefault(); setPlaying(!playing); }
  else if (k === 's') { setPlaying(false); advance(1); }
  else if (k === 'r') randomFill();
  else if (k === 'c') clearAll();
});
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => { readColours(); draw(); });
new ResizeObserver(() => draw()).observe(canvas);

// The page's test hook: tests read and place cells without aiming the mouse.
window.life = {
  cells: () => { const out = []; for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) if (wasm.get_cell(x, y)) out.push([x, y]); return out; },
  place: (name, x, y) => place(name, x, y),
  step: n => advance(n),
};

readColours(); showSpeed(); setRule($('rule').value);
resize(160, 100);
place('gosper', 4, 4);
// Start moving at once, unless the viewer's system asks for less motion.
if (reducedMotion.matches) $('motion').textContent = 'Paused: your system asks for less motion. Press Play to start.';
else setPlaying(true);

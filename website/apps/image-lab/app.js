// Image lab: opens a picture, runs WebAssembly filters on its pixels, and shows before and after.
// The module filters one RGBA buffer in its own memory, in place (see src/lib.rs). This page copies
// the picture in, calls a filter, and draws the same bytes. JavaScript keeps the original, the
// current picture and the undo steps; WebAssembly does every pixel loop.
const { instance } = await WebAssembly.instantiateStreaming(fetch(new URL('./app.wasm', import.meta.url)));
const wasm = instance.exports;

const $ = id => document.getElementById(id);
const stage = $('stage'), after = $('after'), before = $('before'), hist = $('hist');
const MAX_PIXELS = 12e6;          // bigger pictures are scaled down to this, to keep filters quick
const UNDO_BYTES = 200e6;         // the undo steps keep at most this much memory

let img = null;                   // { w, h, name, original: Uint8ClampedArray, current: Uint8ClampedArray }
let undo = [];                    // [{ pixels, label }], oldest first; labels show in History
let pending = null;               // the tool shown as a preview and not applied yet, or null
let buffer = { ptr: 0, len: 0 };  // the pixel buffer inside wasm memory, reused between calls

// ---- The buffer in WebAssembly memory ----

function ensureBuffer(len) {
  if (buffer.len === len) return buffer.ptr;
  if (buffer.len) wasm.dealloc(buffer.ptr, buffer.len);
  buffer = { ptr: wasm.alloc(len), len };
  return buffer.ptr;
}
// Read memory.buffer again every time: a call can grow memory and replace the buffer.
const pixelsInWasm = () => new Uint8ClampedArray(wasm.memory.buffer, buffer.ptr, buffer.len);
function copyIn(pixels) { ensureBuffer(pixels.length); pixelsInWasm().set(pixels); }
function run(filter, ...args) {
  const t0 = performance.now();
  wasm[filter](buffer.ptr, img.w, img.h, ...args);
  return performance.now() - t0;
}

// ---- Drawing ----

function show(ms) {
  after.getContext('2d').putImageData(new ImageData(pixelsInWasm().slice(), img.w, img.h), 0, 0);
  drawHistogram();
  $('pending').hidden = !pending;
  $('info').textContent = `${img.w}×${img.h}` + (ms === undefined ? '' : ` · ${ms.toFixed(0)} ms`);
  $('undo').disabled = !pending && !undo.length;
}

function drawHistogram() {
  const peak = wasm.histogram(buffer.ptr, img.w, img.h);
  const counts = new Uint32Array(wasm.memory.buffer, wasm.histogram_ptr(), 768);
  // Scale to the tallest bar that is not pure black or white: clipped ends would flatten the rest.
  let top = 0;
  for (let c = 0; c < 3; c++) for (let v = 1; v < 255; v++) top = Math.max(top, counts[c * 256 + v]);
  top = top || peak || 1;
  const ratio = Math.min(devicePixelRatio || 1, 2);
  const W = hist.width = Math.round(hist.clientWidth * ratio), H = hist.height = Math.round(hist.clientHeight * ratio);
  const g = hist.getContext('2d');
  g.clearRect(0, 0, W, H);
  ['239,68,68', '34,197,94', '59,130,246'].forEach((rgb, c) => {
    g.beginPath(); g.moveTo(0, H);
    for (let v = 0; v < 256; v++) g.lineTo((v + 0.5) / 256 * W, H - Math.min(1, counts[c * 256 + v] / top) * (H - 4));
    g.lineTo(W, H); g.closePath();
    g.fillStyle = `rgba(${rgb},.28)`; g.fill();
    g.strokeStyle = `rgb(${rgb})`; g.lineWidth = ratio; g.stroke();
  });
  const clipped = (counts[0] + counts[256] + counts[512] + counts[255] + counts[511] + counts[767]) / 3 / (img.w * img.h);
  $('histNote').textContent = clipped > 0.02 ? `${(clipped * 100).toFixed(0)}% pure black or white` : '';
}

function setSplit(pct) {
  pct = Math.max(0, Math.min(100, pct));
  stage.style.setProperty('--split', pct + '%');
  $('split').value = pct;
}

// ---- Opening a picture ----

function open(source, w, h, name) {
  const scale = Math.min(1, Math.sqrt(MAX_PIXELS / (w * h)));
  const sw = Math.max(1, Math.round(w * scale)), sh = Math.max(1, Math.round(h * scale));
  for (const c of [after, before]) { c.width = sw; c.height = sh; }
  const g = before.getContext('2d');
  g.drawImage(source, 0, 0, sw, sh);
  const original = g.getImageData(0, 0, sw, sh).data;
  img = { w: sw, h: sh, name, original, current: original.slice() };
  undo = []; pending = null;
  // The stage keeps the picture's shape and fits the screen's height.
  stage.style.aspectRatio = `${sw} / ${sh}`;
  stage.style.width = `min(100%, calc(74vh * ${sw / sh}))`;
  copyIn(img.current); show(); listSteps();
  if (scale < 1) WardianUI.toast(`Scaled to ${sw}×${sh} so filters stay quick.`, { title: 'Large picture' });
}

async function openFile(file) {
  if (!file || !file.type.startsWith('image/')) { WardianUI.toast('That file is not a picture.', { variant: 'destructive' }); return; }
  try {
    const bitmap = await createImageBitmap(file);
    open(bitmap, bitmap.width, bitmap.height, file.name.replace(/\.[^.]+$/, ''));
    bitmap.close();
  } catch {
    WardianUI.toast('This browser cannot read that picture.', { variant: 'destructive' });
  }
}

// A picture drawn here, so there is something to try at once without a file.
function samplePicture() {
  const c = document.createElement('canvas'); c.width = 1200; c.height = 800;
  const g = c.getContext('2d');
  const sky = g.createLinearGradient(0, 0, 0, 560);
  sky.addColorStop(0, '#1e3a8a'); sky.addColorStop(0.55, '#f97316'); sky.addColorStop(1, '#fde68a');
  g.fillStyle = sky; g.fillRect(0, 0, 1200, 800);
  g.fillStyle = '#fef3c7'; g.beginPath(); g.arc(820, 400, 90, 0, Math.PI * 2); g.fill();
  const ridge = (colour, base, amp, seed) => {
    g.fillStyle = colour; g.beginPath(); g.moveTo(0, 800);
    for (let x = 0; x <= 1200; x += 20) g.lineTo(x, base - amp * (Math.sin(x / 140 + seed) * 0.6 + Math.sin(x / 47 + seed * 3) * 0.25 + Math.sin(x / 13 + seed) * 0.06));
    g.lineTo(1200, 800); g.fill();
  };
  ridge('#7c2d12', 520, 120, 1); ridge('#431407', 600, 90, 4); ridge('#1c1917', 690, 60, 7);
  g.fillStyle = 'rgba(255,255,255,.9)'; g.font = '600 64px system-ui, sans-serif'; g.fillText('Image lab', 60, 120);
  for (let i = 0; i < 6; i++) { g.fillStyle = `hsl(${i * 60} 85% 55%)`; g.fillRect(60 + i * 70, 160, 56, 56); }
  // Film grain and a soft vignette, so it looks more like a photo than flat paint.
  const d = g.getImageData(0, 0, 1200, 800), p = d.data;
  for (let i = 0, y = 0; y < 800; y++) for (let x = 0; x < 1200; x++, i += 4) {
    const dx = x / 600 - 1, dy = y / 400 - 1, shade = 1 - 0.28 * (dx * dx + dy * dy) / 2, grain = (Math.random() - 0.5) * 22;
    for (let k = 0; k < 3; k++) p[i + k] = p[i + k] * shade + grain;
  }
  g.putImageData(d, 0, 0);
  open(c, c.width, c.height, 'sample');
}

// ---- Filters, preview, undo ----

const QUICK = { grayscale: 'Grayscale', sepia: 'Sepia', invert: 'Invert', sobel: 'Edge detect' };
const num = id => Number($(id).value);
// Each tool: how to run it with the current slider values, and how to name it in History.
const TOOLS = {
  bc:        { go: () => run('brightness_contrast', num('p-bright'), num('p-contrast')), label: () => `Brightness ${num('p-bright')}, contrast ${num('p-contrast')}` },
  gauss:     { go: () => run('gaussian_blur', num('p-sigma')), label: () => `Gaussian blur ${num('p-sigma')} px` },
  box:       { go: () => run('box_blur', num('p-radius')), label: () => `Box blur ${num('p-radius')} px` },
  sharpen:   { go: () => run('sharpen', num('p-amount'), num('p-edge')), label: () => `Sharpen ×${num('p-amount')}, ${num('p-edge')} px` },
  posterize: { go: () => run('posterize', num('p-levels')), label: () => `Posterize ${num('p-levels')} levels` },
  threshold: { go: () => run('threshold', num('p-level')), label: () => `Threshold at ${num('p-level')}` },
};

function commit(label) {
  undo.push({ pixels: img.current, label });
  img.current = pixelsInWasm().slice();
  pending = null;
  let bytes = undo.reduce((n, s) => n + s.pixels.length, 0);
  while (undo.length > 1 && bytes > UNDO_BYTES) bytes -= undo.shift().pixels.length;
  listSteps(); show();
}

function listSteps() {
  $('steps').replaceChildren(...undo.map(s => Object.assign(document.createElement('li'), { textContent: s.label })));
  $('steps').scrollTop = $('steps').scrollHeight;
}

let frame = 0;
function preview() {
  // Run at most once per frame while a slider moves.
  if (frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    pending = $('tool').value;
    copyIn(img.current);
    show(TOOLS[pending].go());
  });
}

function discard() {
  if (frame) { cancelAnimationFrame(frame); frame = 0; }
  pending = null;
  copyIn(img.current); show();
}

function applyPending() {
  if (frame) { cancelAnimationFrame(frame); frame = 0; pending = $('tool').value; copyIn(img.current); TOOLS[pending].go(); }
  if (!pending) { pending = $('tool').value; copyIn(img.current); TOOLS[pending].go(); }
  commit(TOOLS[pending].label());
}

function quick(name) {
  if (frame) { cancelAnimationFrame(frame); frame = 0; }
  copyIn(img.current);
  const ms = run(name);
  pending = null;
  commit(QUICK[name]);
  $('info').textContent += ` · ${ms.toFixed(0)} ms`;
}

function stepBack() {
  if (pending) return discard();
  const last = undo.pop();
  if (!last) return;
  img.current = last.pixels;
  copyIn(img.current); listSteps(); show();
}

function reset() {
  if (pending) discard();
  copyIn(img.original);
  commit('Reset to the original');
}

function save() {
  if (pending) applyPending();
  after.toBlob(blob => {
    const a = Object.assign(document.createElement('a'), { href: URL.createObjectURL(blob), download: `${img.name}-edited.png` });
    a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 5000);
  }, 'image/png');
}

// ---- Wiring ----

function showValues() {
  for (const input of document.querySelectorAll('[data-param]')) input.nextElementSibling.textContent = input.value;
}
for (const input of document.querySelectorAll('[data-param]')) input.addEventListener('input', () => { showValues(); preview(); });
$('tool').addEventListener('change', () => {
  for (const p of document.querySelectorAll('.params')) p.hidden = p.dataset.tool !== $('tool').value;
  if (pending) discard();
});
for (const b of document.querySelectorAll('[data-quick]')) b.addEventListener('click', () => quick(b.dataset.quick));
$('apply').addEventListener('click', applyPending);
$('discard').addEventListener('click', discard);
$('undo').addEventListener('click', stepBack);
$('reset').addEventListener('click', reset);
$('save').addEventListener('click', save);
$('sample').addEventListener('click', samplePicture);
$('file').addEventListener('change', e => { openFile(e.target.files[0]); e.target.value = ''; });

// Before and after: the "before" canvas sits on top, cut off at the divider.
$('compare').addEventListener('change', e => stage.classList.toggle('whole', !e.target.checked));
$('split').addEventListener('input', e => setSplit(Number(e.target.value)));
const splitAt = e => { const r = stage.getBoundingClientRect(); setSplit((e.clientX - r.left) / r.width * 100); };
stage.addEventListener('pointerdown', e => { if (stage.classList.contains('whole')) return; stage.setPointerCapture(e.pointerId); splitAt(e); });
stage.addEventListener('pointermove', e => { if (stage.hasPointerCapture(e.pointerId)) splitAt(e); });

// Drop a picture anywhere on the page.
addEventListener('dragover', e => { e.preventDefault(); document.body.classList.add('dragging'); });
addEventListener('dragleave', e => { if (!e.relatedTarget) document.body.classList.remove('dragging'); });
addEventListener('drop', e => { e.preventDefault(); document.body.classList.remove('dragging'); openFile(e.dataTransfer.files[0]); });
addEventListener('keydown', e => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && !(e.target instanceof HTMLInputElement && e.target.type !== 'range')) { e.preventDefault(); stepBack(); }
});
new ResizeObserver(() => img && drawHistogram()).observe(hist);

// The page's test hook: tests open a picture they made, without a file dialog.
window.imageLab = { open: (imageData, name = 'test') => { const c = document.createElement('canvas'); c.width = imageData.width; c.height = imageData.height; c.getContext('2d').putImageData(imageData, 0, 0); open(c, c.width, c.height, name); }, state: () => img && { w: img.w, h: img.h, steps: undo.length, pending } };

showValues(); setSplit(50); samplePicture();

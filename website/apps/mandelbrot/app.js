// Draws the Mandelbrot set with app.wasm. WebAssembly writes RGBA pixels into a buffer in its own
// memory; this page wraps those bytes in an ImageData and puts them on the canvas.
const { instance } = await WebAssembly.instantiateStreaming(fetch(new URL('./app.wasm', import.meta.url)));
const wasm = instance.exports;

const canvas = document.getElementById('view');
const ctx2d = canvas.getContext('2d');
const iterInput = document.getElementById('iter');
const status = document.getElementById('status');

const HOME = { cx: -0.6, cy: 0, width: 3.6 };     // the whole set fits in this view
let view = { ...HOME };
let buffer = { ptr: 0, len: 0 };                    // pixel buffer inside wasm memory, reused

function ensureBuffer(len) {
  if (buffer.len === len) return buffer.ptr;
  if (buffer.len) wasm.dealloc(buffer.ptr, buffer.len);
  buffer = { ptr: wasm.alloc(len), len };
  return buffer.ptr;
}

function draw() {
  // Render at the canvas's real pixel size (capped, so huge screens stay quick).
  const ratio = Math.min(window.devicePixelRatio || 1, 2);
  const w = Math.max(1, Math.round(canvas.clientWidth * ratio));
  const h = Math.max(1, Math.round(canvas.clientHeight * ratio));
  if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }

  const scale = view.width / w;                     // complex units per pixel
  const maxIter = Number(iterInput.value);
  const ptr = ensureBuffer(w * h * 4);
  const t0 = performance.now();
  const inside = wasm.render(ptr, w, h, view.cx, view.cy, scale, maxIter);
  const ms = performance.now() - t0;

  // Read memory.buffer only now: alloc may have grown memory and replaced the buffer.
  const pixels = new Uint8ClampedArray(wasm.memory.buffer, ptr, w * h * 4);
  ctx2d.putImageData(new ImageData(pixels, w, h), 0, 0);

  const zoom = HOME.width / view.width;
  status.replaceChildren(...[
    `centre ${view.cx.toPrecision(12)} ${view.cy >= 0 ? '+' : '−'} ${Math.abs(view.cy).toPrecision(12)}i`,
    `zoom ×${zoom < 1e4 ? zoom.toFixed(zoom < 10 ? 1 : 0) : zoom.toExponential(2)}`,
    `${w}×${h} px in ${ms.toFixed(0)} ms`,
    `${((inside / (w * h)) * 100).toFixed(1)}% inside`,
  ].map(t => Object.assign(document.createElement('span'), { textContent: t })));
  // Doubles run out of digits near ×10^13; say so instead of drawing noise silently.
  if (zoom > 1e13) status.append(Object.assign(document.createElement('span'), { textContent: 'at the limit of 64-bit precision' }));
}

function zoomAt(clientX, clientY, factor) {
  const r = canvas.getBoundingClientRect();
  // The clicked point, in complex coordinates, becomes the new centre.
  view.cx += ((clientX - r.left) / r.width - 0.5) * view.width;
  view.cy += ((clientY - r.top) / r.height - 0.5) * view.width * (r.height / r.width);
  view.width /= factor;
  draw();
}
const zoomOut = () => { view.width = Math.min(view.width * 4, HOME.width * 4); draw(); };
const reset = () => { view = { ...HOME }; draw(); };

canvas.addEventListener('click', e => e.shiftKey ? zoomOut() : zoomAt(e.clientX, e.clientY, 4));
canvas.addEventListener('contextmenu', e => { e.preventDefault(); zoomOut(); });
document.getElementById('out').addEventListener('click', zoomOut);
document.getElementById('reset').addEventListener('click', reset);
iterInput.addEventListener('input', () => { document.getElementById('iterOut').textContent = iterInput.value; draw(); });
addEventListener('keydown', e => {
  if (e.target instanceof HTMLInputElement) return;
  if (e.key === '+' || e.key === '=') { const r = canvas.getBoundingClientRect(); zoomAt(r.left + r.width / 2, r.top + r.height / 2, 4); }
  else if (e.key === '-') zoomOut();
  else if (e.key === 'r' || e.key === 'R') reset();
});
document.getElementById('save').addEventListener('click', () => canvas.toBlob(blob => {
  const a = Object.assign(document.createElement('a'), { href: URL.createObjectURL(blob), download: 'mandelbrot.png' });
  a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 5000);
}));
new ResizeObserver(() => draw()).observe(canvas);

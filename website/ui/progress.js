/* <wardian-progress>: the standard progress bar for Wardian apps.
   Wardian puts it in every suite frame and in /sdk/wardian.js, so an app only writes the tag:

     <wardian-progress label="Searching Splunk" elapsed></wardian-progress>

   Without a value it shows work of unknown length (a moving stripe); with one it fills. From script:

     const bar = ctx.$('wardian-progress');
     bar.start('Searching Splunk');            // shows it, unknown length, starts the clock
     bar.update({value: 2, max: 4, label: 'Step 2 of 4: reading fields'});
     bar.done('8 rows');                        // or bar.fail('Search failed'); bar.hidden = true hides it

   Attributes: label, detail, value, max (default 100), elapsed (show a running clock),
   cancelable (show a Stop button; the element fires a "cancel" event), state (running|done|error).
   Colours: set --wardian-progress-color, --wardian-progress-track, --wardian-progress-error on it or a parent.
   It is a progressbar for screen readers, and it stops moving when the viewer asks for reduced motion. */
(() => {
  'use strict';
  if (typeof customElements === 'undefined' || customElements.get('wardian-progress')) return;

  const CSS = `
    :host { display: block; margin: 10px 0; font-size: 13px; line-height: 1.4;
      --c: var(--wardian-progress-color, #2f6f63); --t: var(--wardian-progress-track, rgba(127,127,127,.22));
      --e: var(--wardian-progress-error, #b4531a); }
    @media (prefers-color-scheme: dark) {
      :host { --c: var(--wardian-progress-color, #6cbfad); --e: var(--wardian-progress-error, #f0a46c); } }
    :host([hidden]) { display: none; }
    .top { display: flex; gap: 12px; align-items: baseline; justify-content: space-between; margin-bottom: 5px; }
    .label { font-weight: 600; overflow-wrap: anywhere; }
    .meta { opacity: .75; font-variant-numeric: tabular-nums; white-space: nowrap; }
    .track { position: relative; height: 8px; border-radius: 99px; background: var(--t); overflow: hidden; }
    .fill { position: absolute; inset: 0 auto 0 0; width: 0; border-radius: inherit; background: var(--c); transition: width .25s ease; }
    :host([data-unknown]) .fill { width: 35%; animation: slide 1.4s ease-in-out infinite; }
    :host([state="done"]) .fill { width: 100%; animation: none; }
    :host([state="error"]) .fill { width: 100%; animation: none; background: var(--e); }
    :host([state="error"]) .label { color: var(--e); }
    @keyframes slide { 0% { left: -35%; } 100% { left: 100%; } }
    @media (prefers-reduced-motion: reduce) {
      .fill { transition: none; }
      :host([data-unknown]) .fill { animation: none; left: 0; width: 100%; opacity: .45; } }
    .bottom { display: flex; gap: 12px; align-items: center; justify-content: space-between; margin-top: 5px; }
    .detail { opacity: .75; overflow-wrap: anywhere; }
    .detail:empty { display: none; }
    button { font: inherit; font-size: 12px; cursor: pointer; border-radius: 6px; padding: 2px 10px; margin-left: auto;
      border: 1px solid var(--t); background: transparent; color: inherit; }
    button[hidden] { display: none; }
    .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }`;

  const clock = ms => {
    const s = Math.floor(ms / 1000), h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, ss = String(s % 60).padStart(2, '0');
    return h ? h + ':' + String(m).padStart(2, '0') + ':' + ss : m + ':' + ss;
  };
  const num = v => v === null || v === undefined || v === '' ? null : (isFinite(Number(v)) ? Number(v) : null);

  class WardianProgress extends HTMLElement {
    static get observedAttributes(){ return ['label', 'detail', 'value', 'max', 'elapsed', 'cancelable', 'state']; }

    constructor(){
      super();
      const root = this.attachShadow({mode: 'open'});
      root.innerHTML = '<style>' + CSS + '</style>' +
        '<div class="top"><span class="label" part="label"></span><span class="meta" part="meta"></span></div>' +
        '<div class="track" part="track"><div class="fill" part="fill"></div></div>' +
        '<div class="bottom"><span class="detail" part="detail"></span><button type="button" hidden>Stop</button></div>' +
        '<span class="sr" aria-live="polite"></span>';
      this._q = s => root.querySelector(s);
      this._q('button').addEventListener('click', () => this.dispatchEvent(new Event('cancel', {bubbles: true})));
      this._began = 0; this._timer = null;
    }

    connectedCallback(){
      this.setAttribute('role', 'progressbar');
      this.setAttribute('aria-valuemin', '0');
      if (this.hasAttribute('elapsed') && !this._began && this.state === 'running') this._clock(true);
      this._paint();
    }
    disconnectedCallback(){ this._clock(false); }
    attributeChangedCallback(){ this._paint(); }

    get label(){ return this.getAttribute('label') || ''; }
    set label(v){ this.setAttribute('label', String(v)); }
    get detail(){ return this.getAttribute('detail') || ''; }
    set detail(v){ this.setAttribute('detail', String(v)); }
    /** The amount done, from 0 to max, or null when the length of the work is unknown. */
    get value(){ return num(this.getAttribute('value')); }
    set value(v){ if (num(v) === null) this.removeAttribute('value'); else this.setAttribute('value', String(num(v))); }
    get max(){ const m = num(this.getAttribute('max')); return m && m > 0 ? m : 100; }
    set max(v){ this.setAttribute('max', String(v)); }
    get state(){ const s = this.getAttribute('state'); return s === 'done' || s === 'error' ? s : 'running'; }
    /** Seconds since start(), or since the element appeared with the elapsed attribute. */
    get seconds(){ return this._began ? Math.round(((this._ended || Date.now()) - this._began) / 1000) : 0; }

    /** Shows the bar, clears the old result, and starts the clock. */
    start(label, opts){
      const o = opts || {};
      this.hidden = false;
      this.removeAttribute('state');
      if (label !== undefined) this.label = label;
      this.detail = o.detail || '';
      if (o.max !== undefined) this.max = o.max;
      this.value = o.value === undefined ? null : o.value;
      if (o.elapsed !== false) this.setAttribute('elapsed', '');
      this.toggleAttribute('cancelable', !!o.cancelable);
      this._clock(true);
      return this;
    }
    /** Changes any of value, max, label, detail. A value of null means "length unknown". */
    update(o){
      if (!o || typeof o !== 'object') { this.value = o; return this; }
      if (o.max !== undefined) this.max = o.max;
      if (o.value !== undefined) this.value = o.value;
      if (o.label !== undefined) this.label = o.label;
      if (o.detail !== undefined) this.detail = o.detail;
      return this;
    }
    done(label){ return this._finish('done', label); }
    fail(label){ return this._finish('error', label); }

    _finish(state, label){
      this._clock(false, true);
      this.removeAttribute('cancelable');
      if (label !== undefined) this.label = label;
      this.setAttribute('state', state);
      this._q('.sr').textContent = this.label;       // announce the end, not every tick
      return this;
    }
    _clock(on, keep){
      clearInterval(this._timer); this._timer = null;
      if (on){ this._began = Date.now(); this._ended = 0; this._timer = setInterval(() => this._paint(), 1000); }
      else if (keep && this._began) this._ended = Date.now();
    }
    _paint(){
      if (!this._q) return;
      const v = this.value, max = this.max, unknown = v === null && this.state === 'running';
      const pct = v === null ? null : Math.max(0, Math.min(100, v / max * 100));
      this.toggleAttribute('data-unknown', unknown);
      this._q('.fill').style.width = unknown || this.state !== 'running' ? '' : pct + '%';
      this._q('.label').textContent = this.label;
      this._q('.detail').textContent = this.detail;
      this._q('button').hidden = !this.hasAttribute('cancelable') || this.state !== 'running';
      const time = this.hasAttribute('elapsed') && this._began ? clock((this._ended || Date.now()) - this._began) : '';
      const share = pct === null || this.state !== 'running' ? '' : Math.floor(pct) + '%';
      this._q('.meta').textContent = [share, time].filter(Boolean).join(' · ');
      this.setAttribute('aria-label', this.label || 'Progress');
      this.setAttribute('aria-valuemax', String(max));
      if (pct === null || this.state !== 'running') this.removeAttribute('aria-valuenow');
      else this.setAttribute('aria-valuenow', String(Math.min(max, Math.max(0, v))));
      this.setAttribute('aria-busy', String(this.state === 'running'));
      this.setAttribute('aria-valuetext', [this.label, share, this.state === 'running' ? '' : this.state].filter(Boolean).join(', '));
    }
  }
  customElements.define('wardian-progress', WardianProgress);
})();

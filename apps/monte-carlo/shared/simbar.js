/* SimBar: shows the simulation's progress (the sim:progress topic) on a <wardian-progress> bar,
   with a Stop button. Used by `tasks` and `forecast`, so either panel can stop a run.
   SimBar.attach(ctx, bar) listens for the bar's Stop and calls sim.cancel; SimBar.show(bar, p)
   draws one progress message. Where <wardian-progress> is not defined, the element stays plain. */
const SimBar = (() => {
  'use strict';
  const where = p => p.mode === 'main' ? ' on the main thread (workers are blocked here)' : ' in a background worker';
  function show(bar, p){
    if (!bar || typeof bar.start !== 'function' || !p) return;
    const trials = Number(p.trials) || 0, done = Number(p.done) || 0;
    if (p.state === 'running'){
      if (bar.hidden || bar.state !== 'running' || bar.dataset.seed !== String(p.seed) || bar.dataset.trials !== String(trials)){
        bar.start('Simulating ' + Fmt.int(trials) + ' trials', {value: done, max: trials || 1, cancelable: true});
        bar.dataset.seed = String(p.seed); bar.dataset.trials = String(trials);
      }
      bar.update({value: done, detail: Fmt.int(done) + ' of ' + Fmt.int(trials) + ' trials' + where(p) + ' · seed ' + p.seed});
    } else if (p.state === 'done'){
      if (bar.hidden) bar.start('', {});
      bar.done(Fmt.int(trials) + ' trials in ' + Fmt.seconds(p.ms || 0));
      bar.update({detail: 'Ran' + where(p) + ' · seed ' + p.seed});
      bar.dataset.seed = '';
    } else if (p.state === 'stopped'){
      if (bar.hidden) bar.start('', {});
      bar.done('Stopped after ' + Fmt.int(done) + ' of ' + Fmt.int(trials) + ' trials');
      bar.update({detail: 'The results use the trials done so far. Run again for all of them.'});
      bar.dataset.seed = '';
    } else if (p.state === 'error'){
      if (bar.hidden) bar.start('', {});
      bar.fail('The simulation failed');
      bar.update({detail: p.error || ''});
      bar.dataset.seed = '';
    } else bar.hidden = true;
  }
  function attach(ctx, bar, say){
    if (!bar) return;
    bar.addEventListener('cancel', () => {
      ctx.call('sim', 'cancel', {}).catch(e => say && say('Could not stop the run: ' + Fmt.errText(e)));
    });
  }
  return Object.freeze({show, attach});
})();

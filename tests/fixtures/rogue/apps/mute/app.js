// Listens to nothing, and declares a method it never provides. It counts every topic message the
// kernel sends it (SPEC 6.7.2: there must be none) and calls late.slow, whose answer the probe forges.
Kernel.register({ name: 'mute', provides: ['never'], needs: ['late.slow'], init(ctx){
  window.msgs = [];
  addEventListener('message', e => { if (e.data && e.data.k === 'msg') window.msgs.push(e.data.topic); });
  ctx.call('late', 'slow').then(v => { window.slowAnswer = v; }, e => { window.slowAnswer = 'error: ' + e.message; });
}});

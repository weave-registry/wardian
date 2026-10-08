// Starts late: its methods are provided only after a second (SPEC 6.7.5). slow answers after 3 s,
// so another frame has time to forge the answer (SPEC 6.7.4).
Kernel.register({ name: 'late', provides: ['ping', 'slow'], listens: ['probe:data'], init(ctx){
  ctx.on('probe:data', p => { window.gotData = p; });
  setTimeout(() => ctx.provide({
    ping: () => 'pong',
    slow: () => new Promise(r => setTimeout(() => r('real'), 3000)),
  }), 1000);
}});

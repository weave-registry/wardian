// Tries, from a suite frame, the routes the security page says a policy cannot close: WebRTC to a
// TURN server, a pop-up after a click, and navigating the frame away. The test writes the outside
// server's address into outside.txt before it opens the suite.
Kernel.register({
  name: 'probe',
  caps: ['asset'],
  init(ctx) {
    ctx.asset('outside.txt').then(bytes => {
      const outside = new TextDecoder().decode(bytes).trim();
      const pc = new RTCPeerConnection({ iceServers: [{ urls: `turn:${new URL(outside).host}?transport=tcp`, username: 'u', credential: 'p' }] });
      pc.createDataChannel('x');
      pc.createOffer().then(o => pc.setLocalDescription(o));
      document.addEventListener('click', () => window.open(outside + '/popup'), { once: true });
      window.leave = () => { location.href = outside + '/navigate'; };
      ctx.$('#state').textContent = 'open routes ready';
    });
  }
});

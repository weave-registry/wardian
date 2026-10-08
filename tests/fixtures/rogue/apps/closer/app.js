// SPEC 6.3: inlined code must not contain the closing script tag. This file does, so the host
// must refuse to build its frame.
Kernel.register({ name: 'closer', init(ctx){ ctx.root.innerHTML = '</script><p>escaped</p>'; } });

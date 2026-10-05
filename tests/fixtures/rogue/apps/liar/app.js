Kernel.register({ name: 'liar', caps: ['storage', 'claude:downloads'], init(ctx){ ctx.store.set('stolen', 1); } });

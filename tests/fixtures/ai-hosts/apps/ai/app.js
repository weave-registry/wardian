/* ai: the out-of-sight AI (ADR-2610131300). Its skill and tools are apps/ai/skill.md and
   apps/ai/tools.json; this code holds only each tool's run. */
const HOSTS = [{ host: 'web-1', errors: 40 }, { host: 'web-2', errors: 3 }, { host: 'db-1', errors: 12 }];
Kernel.register({
  name: 'ai',
  provides: ['ask', 'chat'],
  caps: ['claude:sample', 'asset'],
  init(ctx) {
    const ai = WardianUI.agentApp(ctx, { run: {
      top_hosts: async ({ limit }) => HOSTS.slice().sort((a, b) => b.errors - a.errors).slice(0, limit),
    } });
    ctx.provide({ ask: ({ task }) => ai.ask(task), chat: ({ messages }) => ai.chat(messages) });
  },
});

/* log: the one owner of check-ins, and of the clock. No other part writes them: parts ask through
   set, forget and restore. It also watches for midnight, so every view moves to the new day while
   the app stays open.
   Provides: set, forget, restore.  Emits: log:changed (retained: { today, days }).  Capabilities: storage. */
Kernel.register({
  name: 'log',
  provides: ['set', 'forget', 'restore'],
  emits: { 'log:changed': { retain: true } },
  caps: ['storage'],
  init(ctx) {
    let days = {};
    try { days = Model.days(ctx.store.get('days') || {}); } catch { days = {}; }
    let today = Days.today();

    const publish = () => ctx.emit('log:changed', { today, days });
    const save = () => { ctx.store.set('days', days); publish(); };

    ctx.provide({
      /** Ticks (done: true) or unticks one habit on one day. A day after today is refused. */
      set({ habit, day, done } = {}) {
        if (typeof habit !== 'string' || !habit) throw new Error('No habit given.');
        if (!Days.valid(day)) throw new Error('Not a real day: ' + day);
        if (day > Days.today()) throw new Error('You cannot tick a day that has not come yet.');
        const list = new Set(days[habit] || []);
        if (done) list.add(day); else list.delete(day);
        days = { ...days, [habit]: [...list].sort() };
        save();
        return { habit, day, done: !!done };
      },
      /** Removes every check-in of a habit that was deleted. */
      forget({ habit } = {}) {
        if (!(habit in days)) return { removed: 0 };
        const removed = days[habit].length;
        days = { ...days }; delete days[habit];
        save();
        return { removed };
      },
      /** Replaces all check-ins, from a backup. The caller has checked it; it is checked again here. */
      restore({ days: incoming } = {}) {
        days = Model.days(incoming || {});
        save();
        return { checkins: Model.count(days) };
      },
    });

    // The day can change while the app is open: at midnight, after the computer sleeps, or when the
    // viewer travels. Check at midnight, every minute, and whenever the page comes back into view.
    const tick = () => { const t = Days.today(); if (t !== today) { today = t; publish(); } };
    const atMidnight = () => setTimeout(() => { tick(); atMidnight(); }, Days.msToMidnight() + 1000);
    atMidnight();
    setInterval(tick, 60000);
    document.addEventListener('visibilitychange', tick);
    window.addEventListener('focus', tick);
    publish();
  }
});

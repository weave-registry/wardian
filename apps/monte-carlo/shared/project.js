/* Project: the task list, checked. Used by `tasks` (to show problems as you type) and by the
   simulation, inside the worker too (so it has no DOM and no Kernel calls).
   A task is {id, name, o, m, p, after: [ids]}: optimistic, most likely and pessimistic durations,
   and the tasks that must finish before it starts. */
const Project = (() => {
  'use strict';
  const MAX_TASKS = 200;

  // A small software release. Durations in days.
  const SAMPLE = [
    ['Requirements', 3, 5, 10, []],
    ['Design', 4, 6, 12, [1]],
    ['Backend', 8, 12, 25, [2]],
    ['Frontend', 6, 10, 20, [2]],
    ['Data migration', 2, 4, 12, [2]],
    ['Integration', 3, 5, 10, [3, 4, 5]],
    ['Security review', 2, 3, 8, [3]],
    ['Testing', 4, 6, 14, [6, 7]],
    ['User guide', 2, 3, 6, [4]],
    ['Launch', 1, 1, 3, [8, 9]],
  ];
  function sample(){
    return SAMPLE.map(([name, o, m, p, after], i) => ({id: 't' + (i + 1), name, o, m, p, after: after.map(n => 't' + n)}));
  }

  /* Checks the list. Returns {ok, problem, rows: [message per task or ''], order: [task indexes,
     every task after the tasks it waits for]}. */
  function check(tasks){
    const rows = tasks.map(() => '');
    const index = new Map(tasks.map((t, i) => [t.id, i]));
    if (!tasks.length) return {ok: false, problem: 'Add at least one task.', rows, order: []};
    if (tasks.length > MAX_TASKS) return {ok: false, problem: 'Use at most ' + MAX_TASKS + ' tasks.', rows, order: []};
    tasks.forEach((t, i) => {
      const nums = [t.o, t.m, t.p];
      if (!nums.every(v => typeof v === 'number' && isFinite(v))) rows[i] = 'Enter all three durations.';
      else if (t.o < 0) rows[i] = 'Durations cannot be negative.';
      else if (!(t.o <= t.m && t.m <= t.p)) rows[i] = 'Use optimistic ≤ likely ≤ pessimistic.';
      else if (t.after.some(id => !index.has(id))) rows[i] = 'It waits for a task that is not in the list.';
      else if (t.after.includes(t.id)) rows[i] = 'A task cannot wait for itself.';
    });
    // Topological order (Kahn). Tasks left over sit on a loop.
    const waiting = tasks.map(t => t.after.filter(id => index.has(id) && id !== t.id).length);
    const next = tasks.map(() => []);
    tasks.forEach((t, i) => t.after.forEach(id => { if (index.has(id) && id !== t.id) next[index.get(id)].push(i); }));
    const order = [], queue = [];
    waiting.forEach((w, i) => { if (!w) queue.push(i); });
    while (queue.length){
      const i = queue.shift(); order.push(i);
      for (const j of next[i]) if (--waiting[j] === 0) queue.push(j);
    }
    if (order.length < tasks.length){
      tasks.forEach((t, i) => { if (waiting[i] > 0 && !rows[i]) rows[i] = 'This task is part of a loop: it ends up waiting for itself.'; });
    }
    const bad = rows.findIndex(r => r);
    const problem = bad < 0 ? '' : 'Task ' + (bad + 1) + ' (' + (tasks[bad].name || 'no name') + '): ' + rows[bad];
    return {ok: bad < 0, problem, rows, order};
  }

  /* Finish time of the whole project when every task takes `dur(task)`, in the given order. */
  function finish(tasks, order, dur){
    const index = new Map(tasks.map((t, i) => [t.id, i]));
    const end = new Array(tasks.length).fill(0);
    let last = 0;
    for (const i of order){
      let start = 0;
      for (const id of tasks[i].after) start = Math.max(start, end[index.get(id)]);
      end[i] = start + dur(tasks[i]);
      last = Math.max(last, end[i]);
    }
    return last;
  }

  return Object.freeze({sample, check, finish, MAX_TASKS});
})();

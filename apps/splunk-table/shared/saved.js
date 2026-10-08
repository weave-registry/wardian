/* Saved: the lists of saved searches (search part) and saved tables (keep part) look and work alike.
   This builds their rows and reads the stored lists; each part keeps its own list in its own storage. */
const Saved = Object.freeze({
  newId: () => Date.now().toString(36) + Math.random().toString(36).slice(2, 6),

  // The list stored under `key`, keeping only well-formed entries (older or damaged data is skipped).
  list(store, key){
    const v = store.get(key);
    return Array.isArray(v) ? v.filter(x => x && typeof x === 'object' && typeof x.id === 'string' && typeof x.name === 'string') : [];
  },

  // Asks for a name; returns it, or null when the person cancels or types nothing.
  askName(question, suggested, onEmpty){
    const v = window.prompt(question, suggested || '');
    if (v === null) return null;
    const name = v.trim().replace(/\s+/g, ' ').slice(0, 120);
    if (!name){ if (onEmpty) onEmpty('Type a name to save it under.'); return null; }
    return name;
  },

  // One line in a list: name, a short detail, and small buttons [[label, fn, variant]].
  item(title, detail, buttons){
    const el = tag => document.createElement(tag);
    const li = el('li'), what = el('div'), b = el('b'), s = el('small');
    what.className = 'what'; b.textContent = title; b.title = title; s.textContent = detail;
    what.append(b, s); li.appendChild(what);
    for (const [label, fn, variant] of buttons){
      const btn = el('button');
      btn.type = 'button'; btn.className = 'w-button'; btn.dataset.variant = variant || 'outline'; btn.dataset.size = 'sm';
      btn.textContent = label; btn.setAttribute('aria-label', label + ' ' + title);
      btn.addEventListener('click', fn);
      li.appendChild(btn);
    }
    return li;
  },
  empty(text){ const li = document.createElement('li'); li.className = 'empty'; li.textContent = text; return li; },
});

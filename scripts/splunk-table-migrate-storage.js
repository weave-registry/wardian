#!/usr/bin/env node
// Moves the Splunk table's saved data to the parts that now own it (ADR-2610080900).
//
//   node scripts/splunk-table-migrate-storage.js <DATA_DIR>/state/apps/splunk-table.json
//
// Before, one part ("table") kept everything: {table: {state, savedSearches, savedTables, "savedRows:<id>"}}.
// Now the "search" part keeps the form, the current table and the saved searches (state,
// savedSearches), and the "keep" part keeps the saved tables and their rows (savedTables, savedRows:…).
// The file is first copied to <file>.bak (or <file>.<time>.bak if that exists). Lists are merged by
// id; any other key a part already has is kept, and the old copy stays under "table". Running it
// twice does no harm. Stop Wardian first (or close the app): the server and browser
// write this file too. Saved tables in the database ("saved_…") stay where they are.
'use strict';
const fs = require('fs');

const file = process.argv[2];
if (!file || process.argv.length > 3) {
  console.error('usage: node scripts/splunk-table-migrate-storage.js <DATA_DIR>/state/apps/splunk-table.json');
  process.exit(2);
}
let doc;
try { doc = JSON.parse(fs.readFileSync(file, 'utf8')); }
catch (e) { console.error('cannot read ' + file + ': ' + e.message); process.exit(1); }
if (!doc || typeof doc !== 'object' || Array.isArray(doc)) { console.error(file + ' is not the saved data of an app'); process.exit(1); }

const old = doc.table;
if (!old || typeof old !== 'object' || Array.isArray(old)) {
  console.log('nothing to move: ' + file + ' has no data under "table" (already moved, or never used)');
  process.exit(0);
}

// Which part now owns each old key.
const owner = key => key === 'state' || key === 'savedSearches' ? 'search'
  : key === 'savedTables' || key.startsWith('savedRows:') ? 'keep' : null;

const isList = v => Array.isArray(v);
function mergeList(have, add) {
  const ids = new Set(have.filter(x => x && typeof x.id === 'string').map(x => x.id));
  return have.concat(add.filter(x => !(x && typeof x.id === 'string' && ids.has(x.id))));
}

// A backup from an earlier run is never overwritten.
const bak = fs.existsSync(file + '.bak') ? file + '.' + Date.now() + '.bak' : file + '.bak';
fs.copyFileSync(file, bak);
const moved = [], kept = [], left = {};
for (const [key, value] of Object.entries(old)) {
  const part = owner(key);
  if (!part) { left[key] = value; continue; }
  const to = doc[part] = (doc[part] && typeof doc[part] === 'object' && !Array.isArray(doc[part])) ? doc[part] : {};
  if (!(key in to) || to[key] === null) { to[key] = value; moved.push(part + '.' + key); }
  else if (isList(to[key]) && isList(value)) { to[key] = mergeList(to[key], value); moved.push(part + '.' + key + ' (merged)'); }
  else { kept.push(part + '.' + key); left[key] = value; }
}
// Anything not moved stays under "table", so nothing is lost.
if (Object.keys(left).length) doc.table = left; else delete doc.table;

// Written whole, then renamed, so a crash cannot leave half a file; private, like the server writes it.
const tmp = file + '.tmp';
fs.writeFileSync(tmp, JSON.stringify(doc), { mode: 0o600 });
fs.renameSync(tmp, file);
console.log('backup: ' + bak);
console.log('moved: ' + (moved.join(', ') || 'nothing'));
if (kept.length) console.log('the part already has its own, so the old copy stays under "table": ' + kept.join(', '));
if (Object.keys(left).length) console.log('left under "table": ' + Object.keys(left).join(', '));

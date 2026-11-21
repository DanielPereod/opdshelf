// The highlights store (static/app/notes.js) against a fake server that merges like
// src/annotations.rs (times corrected with `sent_at`, the newest change wins). Whatever the
// difference between the clocks, or the time a request takes, a second edit made while the first
// one is being sent must not disappear. Run: node tests/web/notes_clock_test.js
const fs = require('fs');
const path = require('path');
const source = fs.readFileSync(path.join(__dirname, '../../static/app/notes.js'), 'utf8');

async function run(skew, latency) {
  const storage = new Map();
  const sandbox = { window: { addEventListener() {} }, document: { addEventListener() {} } };
  let browserNow = 1_000_000;
  const server = [];
  let release = null;
  const globals = {
    storeGet: (k) => (storage.has(k) ? storage.get(k) : null),
    storeSet: (k, v) => storage.set(k, String(v)),
    opdshelfDevice: () => ({ id: 'webdev', name: 'Web' }),
    t: (s) => s,
    fetch: (url, opts) => new Promise((resolve) => {
      const handledAt = browserNow + skew + latency;
      release = () => {
        if (opts.method === 'PUT') {
          const body = JSON.parse(opts.body);
          const fix = handledAt - body.sent_at;
          for (const a of body.annotations) {
            const fixed = { ...a, updated: Math.min(handledAt, a.updated + fix) };
            const i = server.findIndex((x) => x.id === a.id);
            if (i < 0) server.push(fixed);
            else if (fixed.updated > server[i].updated) server[i] = fixed;
          }
        }
        resolve({ ok: true, json: async () => ({ server_time: browserNow + skew, annotations: JSON.parse(JSON.stringify(server)) }) });
      };
    }),
  };
  Date.now = () => browserNow * 1000;
  const names = Object.keys(globals).concat(['window', 'document']);
  new Function(...names, source)(...names.map((n) => globals[n] || sandbox[n]));
  const store = sandbox.window.Notes.store('doc1');
  // Opening the book learns the server's clock
  const loading = store.load();
  release();
  await loading;
  const a = store.save({ text: 'hello', note: 'first' });
  const sending = store.flush();
  browserNow += 1;
  store.save({ ...store.get(a.id), note: 'second' });
  release();
  await sending;
  const pending = Object.keys(JSON.parse(storage.get('opdshelf:notes:doc1')).pending).length;
  return { shown: store.get(a.id).note, pending, onServer: server[0].note };
}

(async () => {
  let failed = 0;
  for (const [skew, latency] of [[0, 0], [2, 0], [0, 2], [300, 0], [-120, 0], [-3, 3]]) {
    const result = await run(skew, latency);
    const ok = result.shown === 'second' && result.pending === 1;
    if (!ok) failed++;
    console.log(`${ok ? 'ok' : 'FAILED'} - clock ${skew > 0 ? '+' : ''}${skew} s, request ${latency} s: ${JSON.stringify(result)}`);
  }
  if (failed) process.exit(1);
})();

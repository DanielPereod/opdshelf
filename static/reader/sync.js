// Reading position shared with other devices through /api/progress.
//
// The position is also kept in this browser, so books open where they were
// left while offline. Positions saved offline are sent when the connection is
// back, with the time they were read at, so they do not replace newer reading
// done meanwhile on another device. While a book is open, positions saved on
// other devices arrive live through /api/progress/events.
(function () {
  function deviceId() {
    let id = storeGet('opdshelf:device-id');
    if (!id) {
      id = (crypto.randomUUID ? crypto.randomUUID() : String(Math.random()).slice(2) + Date.now()).replace(/-/g, '');
      storeSet('opdshelf:device-id', id);
    }
    return id;
  }

  function deviceName() {
    const ua = navigator.userAgent;
    const browser = /Edg\//.test(ua) ? 'Edge'
      : /Firefox\//.test(ua) ? 'Firefox'
        : /Chrome\//.test(ua) ? 'Chrome'
          : /Safari\//.test(ua) ? 'Safari' : 'Browser';
    const os = /Android/.test(ua) ? 'Android'
      : /iPhone|iPad/.test(ua) ? 'iOS'
        : /Mac OS X/.test(ua) ? 'macOS'
          : /Windows/.test(ua) ? 'Windows'
            : /Linux/.test(ua) ? 'Linux' : '';
    return os ? `${browser} on ${os}` : browser;
  }

  function toast(message, action) {
    document.querySelectorAll('.reader-toast').forEach((t) => t.remove());
    const el = document.createElement('div');
    el.className = 'reader-toast';
    el.setAttribute('role', 'status');
    const text = document.createElement('span');
    text.textContent = message;
    el.append(text);
    if (action) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = action.label;
      button.onclick = () => { el.remove(); action.run(); };
      el.append(button);
    }
    document.getElementById('reader').append(el);
    setTimeout(() => el.remove(), action ? 10000 : 4000);
    return el;
  }

  // The sync icon in the reader's bar: saved, waiting to be sent, offline or live
  const labels = {
    saved: () => t('Position saved on the server'),
    pending: () => t('Saving the position…'),
    offline: () => t('Offline: the position is kept here and sent when the connection is back'),
    live: () => t('Synced and following other devices live'),
    off: () => t('Not synced: this book has no id'),
  };
  const icons = {
    saved: 'fas fa-cloud', pending: 'fas fa-cloud-arrow-up', offline: 'fas fa-cloud-bolt',
    live: 'fas fa-cloud', off: 'fas fa-cloud',
  };
  function showState(state) {
    const el = document.getElementById('syncState');
    if (!el) return;
    el.dataset.state = state;
    el.title = labels[state]();
    el.setAttribute('aria-label', labels[state]());
    el.querySelector('i').className = icons[state];
  }

  function create(documentId) {
    const me = deviceId();
    const device = deviceName();
    const localKey = 'opdshelf:position:' + documentId;
    let timer = null;
    let following = false;
    const settled = () => (navigator.onLine === false ? 'offline' : following ? 'live' : 'saved');
    if (!documentId) showState('off');

    const readLocal = () => {
      try { return JSON.parse(storeGet(localKey)) || null; } catch { return null; }
    };
    const writeLocal = (entry) => storeSet(localKey, JSON.stringify(entry));

    async function request(method, body, keepalive) {
      if (!documentId) throw new Error('no document id');
      const response = await fetch('/api/progress/' + documentId, {
        method,
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: body ? JSON.stringify(body) : undefined,
        credentials: 'same-origin',
        keepalive: Boolean(keepalive),
      });
      if (!response.ok) throw new Error(response.status + ' ' + response.statusText);
      return response.json();
    }

    async function flush(keepalive) {
      clearTimeout(timer);
      const local = readLocal();
      if (!local || !local.pending) return;
      try {
        // With the time it was sent, the server corrects a wrong clock
        await request('PUT', { ...local.record, sent_at: Math.floor(Date.now() / 1000) }, keepalive);
        const now = readLocal();
        // Something newer may have been saved while sending
        if (now && now.record.timestamp === local.record.timestamp) {
          writeLocal({ record: now.record, pending: false });
          showState(settled());
        }
      } catch {
        // Offline: kept until the connection is back
        showState('offline');
      }
    }

    // Where to open the book: { record, from } where `from` names the other
    // device the position comes from, when it is not where this browser was.
    async function load() {
      const local = readLocal();
      let server;
      try {
        server = (await request('GET')).current;
        showState(settled());
      } catch {
        if (documentId) showState('offline');
        return { record: local ? local.record : null, from: null };
      }
      if (local && local.pending) {
        flush();
        if (!server || local.record.timestamp >= server.timestamp) return { record: local.record, from: null };
      }
      if (!server) return { record: local ? local.record : null, from: null };
      writeLocal({ record: server, pending: false });
      const moved = !local || local.record.progress !== server.progress || local.record.cfi !== server.cfi;
      return { record: server, from: server.device_id !== me && moved ? server.device : null };
    }

    // Saves a position: { progress, percentage, cfi?, context?, manual? }
    function save(position) {
      const record = {
        progress: position.progress || '',
        percentage: Math.min(1, Math.max(0, position.percentage || 0)),
        device,
        device_id: me,
        timestamp: Math.floor(Date.now() / 1000),
      };
      if (position.cfi) record.cfi = position.cfi;
      if (position.context) record.context = position.context;
      if (position.manual) record.manual = true;
      writeLocal({ record, pending: true });
      if (documentId) showState('pending');
      clearTimeout(timer);
      if (position.manual) return flush();
      timer = setTimeout(flush, 2000);
      return Promise.resolve();
    }

    // Calls `onRemote(record)` when another device moves this book on.
    function follow(onRemote) {
      if (!documentId || !window.EventSource) return;
      let retry = 2000;
      const query = new URLSearchParams({ documents: documentId, device_id: me, device });
      const connect = () => {
        const source = new EventSource('/api/progress/events?' + query);
        source.addEventListener('hello', () => {
          retry = 2000;
          following = true;
          if (!(readLocal() || {}).pending) showState('live');
        });
        source.addEventListener('progress', (e) => {
          const change = JSON.parse(e.data);
          const record = change.current;
          if (!record || record.device_id === me) return;
          writeLocal({ record, pending: false });
          onRemote(record);
        });
        source.addEventListener('resync', async () => {
          try {
            const record = (await request('GET')).current;
            if (record && record.device_id !== me) onRemote(record);
          } catch { }
        });
        source.onerror = () => {
          source.close();
          following = false;
          showState(navigator.onLine === false ? 'offline' : 'saved');
          setTimeout(connect, retry);
          retry = Math.min(60000, retry * 2);
        };
      };
      connect();
    }

    window.addEventListener('offline', () => showState('offline'));
    window.addEventListener('online', () => flush());
    window.addEventListener('pagehide', () => flush(true));
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') flush(true);
    });

    return { load, save, flush, follow };
  }

  // The message shown when another device moved on, with a button to go there
  function remoteToast(record, go) {
    const pct = Math.round((record.percentage || 0) * 100);
    const from = record.device || t('another device');
    toast(record.manual ? t('{device} set a point here', { device: from })
      : t('{device} is at {pct}%', { device: from, pct }), { label: t('Go there'), run: go });
  }

  window.ReaderSync = { create, toast, remoteToast };
})();

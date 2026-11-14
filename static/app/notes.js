// Highlights and notes of a book, shared with the other devices through
// /api/annotations, and the list that shows them in the panel and the reader.
//
// Changes are kept in this browser first and sent a moment later, so nothing
// is lost while offline. Every highlight has an id made where it was created;
// the server keeps the newest change of each one and tells the other devices.
(function () {
  const COLORS = ['yellow', 'orange', 'red', 'purple', 'blue', 'cyan', 'green', 'olive', 'gray'];
  // The colours offered when highlighting; the others come from KOReader
  const PICK = ['yellow', 'green', 'blue', 'red', 'purple'];
  const NAMES = {
    yellow: 'Yellow', orange: 'Orange', red: 'Red', purple: 'Purple', blue: 'Blue',
    cyan: 'Cyan', green: 'Green', olive: 'Olive', gray: 'Gray',
  };

  const now = () => Math.floor(Date.now() / 1000);
  const newId = () => 'w' + (crypto.randomUUID ? crypto.randomUUID() : String(Math.random()).slice(2) + Date.now()).replace(/-/g, '').slice(0, 24);
  const squeeze = (s) => (s || '').replace(/\s+/g, ' ').trim();

  function byPosition(a, b) {
    return (a.percentage || 0) - (b.percentage || 0) || (a.created || 0) - (b.created || 0) || (a.id < b.id ? -1 : 1);
  }

  // ------------------------------------------------------------------ store

  function store(documentId) {
    const key = 'opdshelf:notes:' + documentId;
    const me = opdshelfDevice();
    const listeners = [];
    let timer = null;
    let sending = null;
    let state;
    try { state = JSON.parse(storeGet(key)) || null; } catch { state = null; }
    if (!state || !Array.isArray(state.list)) state = { list: [], pending: {} };
    const write = () => storeSet(key, JSON.stringify(state));
    const emit = (changed) => listeners.forEach((fn) => fn(changed || null));

    // The server's copy with the changes not sent yet on top
    function merged() {
      const map = new Map(state.list.map((a) => [a.id, a]));
      Object.values(state.pending).forEach((a) => map.set(a.id, a));
      return Array.from(map.values());
    }
    const list = () => merged().filter((a) => !a.deleted).sort(byPosition);
    const get = (id) => merged().find((a) => a.id === id) || null;

    async function request(method, body, keepalive) {
      const response = await fetch('/api/annotations/' + documentId, {
        method,
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        body: body ? JSON.stringify(body) : undefined,
        credentials: 'same-origin',
        keepalive: Boolean(keepalive),
      });
      if (!response.ok) throw new Error(response.status + ' ' + response.statusText);
      return response.json();
    }

    // Takes the server's list, keeping local changes it does not have yet
    function adopt(serverList) {
      state.list = serverList;
      const server = new Map(serverList.map((a) => [a.id, a]));
      Object.values(state.pending).forEach((a) => {
        const s = server.get(a.id);
        if (s && s.updated > a.updated) delete state.pending[a.id];
      });
      write();
    }

    async function load() {
      if (!documentId) return list();
      try {
        const body = await request('GET');
        adopt(body.annotations || []);
        emit();
        flush();
      } catch {
        // Offline: what this browser has
      }
      return list();
    }

    async function flush(keepalive) {
      clearTimeout(timer);
      if (!documentId) return;
      if (sending) return sending;
      const batch = Object.values(state.pending);
      if (!batch.length) return;
      sending = (async () => {
        try {
          const body = await request('PUT', { device: me.name, device_id: me.id, sent_at: now(), annotations: batch }, keepalive);
          // Changed again while sending: those stay queued
          batch.forEach((a) => {
            const p = state.pending[a.id];
            if (p && p.updated === a.updated && p.note === a.note && p.color === a.color && p.deleted === a.deleted) delete state.pending[a.id];
          });
          adopt(body.annotations || []);
          emit();
        } catch {
          timer = setTimeout(flush, 15000);
        } finally {
          sending = null;
        }
      })();
      return sending;
    }

    // Saves a new or changed highlight
    function save(annotation) {
      const before = get(annotation.id);
      const a = {
        color: 'yellow', style: 'lighten', text: '', note: '', pos0: '', pos1: '', chapter: '', percentage: 0,
        ...before, ...annotation,
        id: annotation.id || newId(),
        updated: Math.max(now(), (before && before.updated) || 0),
        device: me.name,
        device_id: me.id,
      };
      if (!a.created) a.created = a.updated;
      state.pending[a.id] = a;
      write();
      emit([a]);
      clearTimeout(timer);
      timer = setTimeout(flush, 800);
      return a;
    }

    function remove(id) {
      const a = get(id);
      if (!a) return null;
      save({ ...a, deleted: true });
      return a;
    }

    // Brings back a highlight that was just deleted
    function restore(a) {
      return save({ ...a, deleted: false });
    }

    // Highlights changed on another device, as announced live
    function applyRemote(changed) {
      let touched = false;
      changed.forEach((a) => {
        const i = state.list.findIndex((x) => x.id === a.id);
        if (i >= 0 && state.list[i].updated > a.updated) return;
        if (i >= 0) state.list[i] = a; else state.list.push(a);
        const p = state.pending[a.id];
        if (p && p.updated <= a.updated && a.device_id !== me.id) delete state.pending[a.id];
        touched = true;
      });
      if (touched) {
        write();
        emit(changed);
      }
    }

    const onChange = (fn) => listeners.push(fn);
    window.addEventListener('online', () => flush());
    window.addEventListener('pagehide', () => flush(true));

    return { load, list, get, save, remove, restore, applyRemote, onChange, flush, documentId };
  }

  // ------------------------------------------------------------------ list view

  function el(tag, className, text) {
    const e = document.createElement(tag);
    if (className) e.className = className;
    if (text !== undefined && text !== null) e.textContent = text;
    return e;
  }

  function icon(name) {
    const i = el('i', name);
    i.setAttribute('aria-hidden', 'true');
    return i;
  }

  function button(className, iconName, label, onClick, title) {
    const b = el('button', className);
    b.type = 'button';
    if (iconName) b.append(icon(iconName));
    if (label) b.append(document.createTextNode(label));
    if (title) { b.title = title; b.setAttribute('aria-label', title); }
    b.addEventListener('click', onClick);
    return b;
  }

  function swatches(current, onPick, colors) {
    const row = el('div', 'hl-swatches');
    row.setAttribute('role', 'group');
    row.setAttribute('aria-label', t('Colour'));
    (colors || PICK).forEach((c) => {
      const b = button('hl-swatch', null, null, () => onPick(c), t(NAMES[c]));
      b.dataset.color = c;
      b.setAttribute('aria-pressed', c === current);
      row.append(b);
    });
    return row;
  }

  function download(text, name, type) {
    const blob = new Blob([text], { type });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = name;
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  // Markdown of what the list shows, for copying
  function markdown(items) {
    let out = '';
    let chapter = null;
    items.forEach((a) => {
      if (a.chapter !== chapter) {
        chapter = a.chapter;
        out += `\n## ${chapter || t('No chapter')}\n`;
      }
      out += '\n' + squeeze(a.text).split('\n').map((l) => '> ' + l).join('\n') + '\n';
      if (a.note && a.note.trim()) out += '\n' + a.note.trim() + '\n';
    });
    return out.trim() + '\n';
  }

  /*
   * Shows the highlights of a store in `root`, with search, filters and export.
   * opts.open(a): go to a highlight (a button is shown when given)
   * opts.toast(message, action): shows a message, with an undo button
   * opts.compact: for the reader's side panel
   */
  function view(root, notes, opts = {}) {
    const ui = { query: '', color: null, onlyNotes: false, sort: 'position', editing: null };
    root.classList.add('hl-view');
    if (opts.compact) root.classList.add('compact');
    root.replaceChildren();

    const head = el('div', 'hl-head');
    const stats = el('p', 'hl-stats');
    const tools = el('div', 'hl-tools');
    const search = el('input', 'input hl-search');
    search.type = 'search';
    search.placeholder = t('Search highlights and notes');
    search.setAttribute('aria-label', t('Search highlights and notes'));
    search.addEventListener('input', () => { ui.query = search.value; paint(); });
    const sort = el('select', 'input hl-sort');
    sort.setAttribute('aria-label', t('Order'));
    [['position', t('In book order')], ['newest', t('Newest first')]].forEach(([value, label]) => {
      const o = el('option', null, label);
      o.value = value;
      sort.append(o);
    });
    sort.addEventListener('change', () => { ui.sort = sort.value; paint(); });
    const exportBtn = button('btn btn-small', 'fas fa-arrow-up-from-bracket', t('Export'), () => {
      menu.hidden = !menu.hidden;
      exportBtn.setAttribute('aria-expanded', !menu.hidden);
    });
    exportBtn.setAttribute('aria-haspopup', 'true');
    exportBtn.setAttribute('aria-expanded', 'false');
    const menu = el('div', 'hl-menu');
    menu.hidden = true;
    const base = '/api/annotations/' + notes.documentId + '/export';
    const link = (href, iconName, label) => {
      const a = el('a', null);
      a.href = href;
      a.append(icon(iconName), document.createTextNode(label));
      a.addEventListener('click', () => { menu.hidden = true; });
      return a;
    };
    menu.append(
      link(base + '?format=markdown', 'fab fa-markdown', t('Markdown file')),
      link(base + '?format=json', 'fas fa-code', t('JSON file')),
      button(null, 'far fa-copy', t('Copy as text'), async () => {
        menu.hidden = true;
        const text = markdown(filtered());
        try {
          await navigator.clipboard.writeText(text);
          say(t('Copied'));
        } catch {
          download(text, 'highlights.md', 'text/markdown');
        }
      }),
    );
    const exportBox = el('div', 'hl-export');
    exportBox.append(exportBtn, menu);
    document.addEventListener('click', (e) => {
      if (!exportBox.contains(e.target)) { menu.hidden = true; exportBtn.setAttribute('aria-expanded', 'false'); }
    });
    tools.append(search, exportBox);

    const filters = el('div', 'hl-filters');
    const statsRow = el('div', 'hl-statsrow');
    statsRow.append(stats, sort);
    head.append(statsRow, tools, filters);
    const body = el('div', 'hl-list');
    body.setAttribute('aria-live', 'polite');
    root.append(head, body);

    const say = (message, action) => (opts.toast ? opts.toast(message, action) : null);

    function filtered() {
      const q = squeeze(ui.query).toLowerCase();
      let items = notes.list().filter((a) => (!ui.color || a.color === ui.color)
        && (!ui.onlyNotes || (a.note && a.note.trim()))
        && (!q || (a.text + ' ' + (a.note || '') + ' ' + (a.chapter || '')).toLowerCase().includes(q)));
      if (ui.sort === 'newest') items = items.slice().sort((a, b) => b.updated - a.updated);
      return items;
    }

    function paintFilters(all) {
      filters.replaceChildren();
      const used = COLORS.filter((c) => all.some((a) => a.color === c));
      const chip = (label, pressed, onClick, color) => {
        const b = button('hl-chip', null, null, onClick);
        if (color) {
          const dot = el('span', 'hl-dot');
          dot.dataset.color = color;
          b.append(dot);
        }
        b.append(document.createTextNode(label));
        b.setAttribute('aria-pressed', pressed);
        filters.append(b);
      };
      chip(t('All'), !ui.color && !ui.onlyNotes, () => { ui.color = null; ui.onlyNotes = false; paint(); });
      chip(t('With a note'), ui.onlyNotes, () => { ui.onlyNotes = !ui.onlyNotes; paint(); });
      if (used.length > 1) {
        used.forEach((c) => chip(t(NAMES[c]), ui.color === c, () => { ui.color = ui.color === c ? null : c; paint(); }, c));
      }
      filters.hidden = all.length === 0;
    }

    function item(a) {
      const li = el('li', 'hl-item');
      li.dataset.color = a.color;
      li.dataset.id = a.id;
      const quote = el('blockquote', 'hl-text', squeeze(a.text) || t('(no text)'));
      if (a.style === 'underscore') quote.classList.add('underline');
      if (a.style === 'strikeout') quote.classList.add('strike');
      li.append(quote);
      if (ui.editing === a.id) {
        const form = el('form', 'hl-edit');
        const area = el('textarea', 'input');
        area.rows = 4;
        area.value = a.note || '';
        area.placeholder = t('Write a note…');
        area.setAttribute('aria-label', t('Note'));
        const row = el('div', 'hl-edit-row');
        let color = a.color;
        const colors = swatches(color, (c) => {
          color = c;
          colors.querySelectorAll('button').forEach((b) => b.setAttribute('aria-pressed', b.dataset.color === c));
        }, PICK.includes(a.color) ? PICK : PICK.concat(a.color));
        const cancel = button('btn btn-small', null, t('Cancel'), () => { ui.editing = null; paint(); });
        const ok = el('button', 'btn btn-small btn-primary', t('Save'));
        ok.type = 'submit';
        row.append(colors, cancel, ok);
        form.append(area, row);
        form.addEventListener('submit', (e) => {
          e.preventDefault();
          ui.editing = null;
          notes.save({ ...a, note: area.value.trim(), color });
          paint();
        });
        area.addEventListener('keydown', (e) => {
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) form.requestSubmit();
          if (e.key === 'Escape') { e.stopPropagation(); ui.editing = null; paint(); }
        });
        li.append(form);
        setTimeout(() => area.focus(), 0);
      } else if (a.note && a.note.trim()) {
        li.append(el('p', 'hl-note', a.note.trim()));
      }
      const foot = el('div', 'hl-foot');
      const meta = [];
      if (a.page) meta.push(t('p. {n}', { n: a.page }));
      meta.push(Math.round((a.percentage || 0) * 100) + '%');
      if (a.device) meta.push(a.device);
      meta.push(timeAgo(a.updated));
      const info = el('small', 'hl-meta', meta.filter(Boolean).join(' · '));
      info.title = new Date(a.created * 1000).toLocaleString(document.documentElement.lang || undefined);
      const actions = el('div', 'hl-actions');
      if (opts.open) actions.append(button('icon-btn', 'fas fa-arrow-right-to-bracket', null, () => opts.open(a), t('Go to the highlight')));
      if (ui.editing !== a.id) {
        actions.append(button('icon-btn', a.note ? 'fas fa-pen' : 'far fa-note-sticky', null, () => { ui.editing = a.id; paint(); },
          a.note ? t('Edit note') : t('Add a note')));
      }
      actions.append(button('icon-btn', 'far fa-copy', null, async () => {
        try {
          await navigator.clipboard.writeText(squeeze(a.text) + (a.note ? '\n\n' + a.note.trim() : ''));
          say(t('Copied'));
        } catch { }
      }, t('Copy')));
      actions.append(button('icon-btn', 'far fa-trash-can', null, () => {
        const gone = notes.remove(a.id);
        say(t('Highlight deleted'), { label: t('Undo'), run: () => notes.restore(gone) });
      }, t('Delete')));
      foot.append(info, actions);
      li.append(foot);
      return li;
    }

    function paint() {
      const all = notes.list();
      const items = filtered();
      const withNotes = all.filter((a) => a.note && a.note.trim()).length;
      stats.textContent = !all.length ? ''
        : all.length === 1 ? (withNotes ? t('1 highlight, with a note') : t('1 highlight'))
          : t('{n} highlights · {m} with a note', { n: all.length, m: withNotes });
      tools.hidden = all.length === 0;
      sort.hidden = all.length < 2;
      paintFilters(all);
      body.replaceChildren();
      if (!all.length) {
        const empty = el('div', 'empty small hl-empty');
        empty.append(icon('fas fa-highlighter'), el('strong', null, t('No highlights yet')),
          el('p', null, t('Select text while reading, on the web, in the app or in KOReader, to highlight it or add a note. They show up here on every device.')));
        body.append(empty);
        return;
      }
      if (!items.length) {
        body.append(el('p', 'muted hl-none', t('Nothing matches.')));
        return;
      }
      let group = null;
      let ul = null;
      items.forEach((a) => {
        const name = ui.sort === 'position' ? (a.chapter || t('No chapter')) : '';
        if (!ul || name !== group) {
          group = name;
          const section = el('section', 'hl-group');
          if (name) section.append(el('h4', null, name));
          ul = el('ul', null);
          section.append(ul);
          body.append(section);
        }
        ul.append(item(a));
      });
    }

    notes.onChange(() => { if (!ui.editing) paint(); });
    paint();
    return { paint, focusSearch: () => search.focus(), scrollTo: (id) => {
      const target = body.querySelector(`[data-id="${CSS.escape(id)}"]`);
      if (target) { target.scrollIntoView({ block: 'center' }); target.classList.add('flash'); }
    } };
  }

  window.Notes = { store, view, swatches, COLORS, PICK, NAMES, newId };
})();

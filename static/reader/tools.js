// Search in the book and bookmarks, for the EPUB and PDF readers.
//
// Each reader gives an adapter that knows its book:
//   sections()                 how many parts to search (chapters, pages)
//   searchSection(i, find)     the hits of part i: find(text) gives the matches of a text
//   showHit(hit) / clearHit()  goes to a hit and marks it, or takes the mark away
//   here()                     a bookmark for the page on screen (may be a promise)
//   isHere(bookmark)           whether a bookmark is on the page on screen
//   goTo(bookmark)
//   where(bookmark)            "Page 4" or "Chapter 2 · 35%", for the list
//
// Bookmarks are kept on the server per book (/api/bookmarks), so they show
// in every browser; the Android app keeps its own on the device.
(function () {
  const LIMIT = 500;
  const squeeze = (s) => (s || '').replace(/\s+/g, ' ').trim();
  // Lower case without accents, char by char, so "camion" finds "Camión"
  const foldChar = (c) => c.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();

  /*
   * A text made searchable: `fold` is the text folded and `map[i]` the index
   * in the original text of each folded character.
   */
  function folded(text) {
    let fold = '';
    const map = [];
    for (let i = 0; i < text.length; i++) {
      const f = foldChar(text[i]);
      for (let j = 0; j < f.length; j++) { fold += f[j]; map.push(i); }
    }
    return { fold, map };
  }

  // The matches of `query` in `text`, as { start, end, before, match, after } in the original text
  function finder(query) {
    const q = folded(squeeze(query)).fold;
    return (text, max) => {
      const hits = [];
      if (!q) return hits;
      const { fold, map } = folded(text);
      let from = 0;
      while (hits.length < max) {
        const at = fold.indexOf(q, from);
        if (at < 0) break;
        const start = map[at];
        const end = map[at + q.length - 1] + 1;
        hits.push({
          start, end,
          before: squeeze(text.slice(Math.max(0, start - 60), start)).replace(/^\S*\s/, '…'),
          match: text.slice(start, end),
          after: squeeze(text.slice(end, end + 80)).replace(/\s\S*$/, '…'),
        });
        from = at + q.length;
      }
      return hits;
    };
  }

  function el(tag, className, text) {
    const e = document.createElement(tag);
    if (className) e.className = className;
    if (text !== undefined && text !== null) e.textContent = text;
    return e;
  }

  function mount(adapter) {
    const isMac = /Mac|iPhone|iPad/.test(navigator.platform || '');
    const searchBtn = document.getElementById('searchBtn');
    const panel = document.getElementById('searchPanel');
    const input = document.getElementById('searchInput');
    const status = document.getElementById('searchStatus');
    const results = document.getElementById('searchResults');
    const markBtn = document.getElementById('bookmarkBtn');
    const listBox = document.getElementById('bookmarkList');
    const countBox = document.getElementById('bookmarkCount');
    const wide = window.matchMedia('(min-width: 761px)');

    searchBtn.title = t('Search in the book') + ' (' + (isMac ? '⌘F' : 'Ctrl+F') + ')';
    searchBtn.setAttribute('aria-label', t('Search in the book'));

    // ------------------------------------------------------------ search

    function showSearch(open) {
      if (open === !panel.hidden) { if (open) input.focus(); return; }
      panel.hidden = !open;
      searchBtn.setAttribute('aria-pressed', open);
      if (open) {
        document.dispatchEvent(new CustomEvent('readerpanel', { detail: { name: 'search' } }));
        setTimeout(() => { input.focus(); input.select(); }, 0);
      } else {
        adapter.clearHit();
        if (document.activeElement === input || panel.contains(document.activeElement)) searchBtn.focus();
      }
      if (adapter.resize) requestAnimationFrame(adapter.resize);
    }
    searchBtn.addEventListener('click', () => showSearch(panel.hidden));
    document.getElementById('searchClose').addEventListener('click', () => showSearch(false));
    document.addEventListener('readerpanel', (e) => { if (e.detail.name !== 'search') showSearch(false); });

    let run = 0;
    let hits = [];
    let current = -1;
    function say(text, icon) {
      status.replaceChildren();
      if (icon) {
        const i = el('i', icon);
        i.setAttribute('aria-hidden', 'true');
        status.append(i, ' ');
      }
      status.append(text);
    }
    function emptyState() {
      results.replaceChildren();
      const box = el('div', 'empty small search-empty');
      const i = el('i', 'fas fa-magnifying-glass');
      i.setAttribute('aria-hidden', 'true');
      box.append(i, el('p', null, t('Type a word or a phrase and press Enter. Accents and capitals do not matter.')));
      results.append(box);
    }
    function pick(i, go) {
      if (i < 0 || i >= hits.length) return;
      results.querySelectorAll('[aria-current="true"]').forEach((b) => b.setAttribute('aria-current', 'false'));
      current = i;
      const button = hits[i].button;
      button.setAttribute('aria-current', 'true');
      button.scrollIntoView({ block: 'nearest' });
      if (go) {
        adapter.showHit(hits[i]);
        if (!wide.matches) showSearch(false);
      }
    }
    function addHit(hit, group) {
      const index = hits.length;
      hits.push(hit);
      const b = el('button', 'search-hit');
      b.type = 'button';
      b.setAttribute('aria-current', 'false');
      const text = el('span', 'search-text');
      text.append(hit.before + ' ', el('mark', null, hit.match), ' ' + hit.after);
      b.append(text);
      b.addEventListener('click', () => pick(index, true));
      hit.button = b;
      group.append(b);
    }
    async function search(query) {
      const mine = ++run;
      hits = [];
      current = -1;
      adapter.clearHit();
      results.replaceChildren();
      query = squeeze(query);
      if (!query) { say(''); emptyState(); return; }
      const find = finder(query);
      const total = adapter.sections();
      let label = null;
      let group = null;
      for (let i = 0; i < total && hits.length < LIMIT; i++) {
        if (mine !== run) return;
        say(t('Searching… {n} of {total}', { n: i + 1, total }), 'fas fa-spinner fa-spin');
        let found = [];
        try { found = await adapter.searchSection(i, (text) => find(text, LIMIT - hits.length)); } catch { found = []; }
        if (mine !== run) return;
        found.forEach((hit) => {
          if (!group || hit.group !== label) {
            label = hit.group;
            const section = el('section', 'search-group');
            if (label) section.append(el('h3', null, label));
            group = el('div', 'search-list');
            section.append(group);
            results.append(section);
          }
          addHit(hit, group);
        });
      }
      if (mine !== run) return;
      if (!hits.length) {
        say('');
        const box = el('div', 'empty small search-empty');
        const i = el('i', 'far fa-face-meh');
        i.setAttribute('aria-hidden', 'true');
        box.append(i, el('strong', null, t('No matches')), el('p', null, t('Nothing in this book matches “{query}”.', { query })));
        results.append(box);
        return;
      }
      say(hits.length >= LIMIT ? t('The first {n} matches', { n: LIMIT })
        : hits.length === 1 ? t('1 match') : t('{n} matches', { n: hits.length }));
    }
    document.getElementById('searchForm').addEventListener('submit', (e) => {
      e.preventDefault();
      // Enter again goes on to the next match
      if (hits.length && input.value === input.dataset.searched) { pick((current + 1) % hits.length, true); return; }
      input.dataset.searched = input.value;
      search(input.value).then(() => { if (hits.length && wide.matches) pick(0, true); });
    });
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.stopPropagation(); showSearch(false); }
      if (e.key === 'ArrowDown' && hits.length) { e.preventDefault(); pick(Math.min(hits.length - 1, current + 1), true); }
      if (e.key === 'ArrowUp' && hits.length) { e.preventDefault(); pick(Math.max(0, current - 1), true); }
    });
    emptyState();

    // Ctrl or Cmd+F searches in the book instead of in the page around it
    function onKey(e) {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === 'f' || e.key === 'F')) {
        e.preventDefault();
        showSearch(true);
        return true;
      }
      if (e.key === 'Escape' && !panel.hidden && !document.querySelector('dialog[open]')) { showSearch(false); return true; }
      return false;
    }
    document.addEventListener('keydown', onKey);

    // ------------------------------------------------------------ bookmarks

    const marks = Notes.store(adapter.documentId, { kind: 'bookmarks' });
    let here = null;
    function paintButton() {
      here = marks.list().find((b) => adapter.isHere(b)) || null;
      markBtn.setAttribute('aria-pressed', Boolean(here));
      const label = here ? t('Remove the bookmark of this page') : t('Bookmark this page');
      markBtn.title = label;
      markBtn.setAttribute('aria-label', label);
      markBtn.querySelector('i').className = here ? 'fas fa-bookmark' : 'far fa-bookmark';
    }
    function paintList() {
      const list = marks.list();
      if (countBox) {
        countBox.textContent = list.length ? String(list.length) : '';
        countBox.hidden = !list.length;
      }
      listBox.replaceChildren();
      if (!list.length) {
        const box = el('div', 'empty small bm-empty');
        const i = el('i', 'far fa-bookmark');
        i.setAttribute('aria-hidden', 'true');
        box.append(i, el('strong', null, t('No bookmarks yet')),
          el('p', null, t('Use the bookmark button at the top to keep a page and come back to it later.')));
        listBox.append(box);
        return;
      }
      const ul = el('ul', 'bm-list');
      list.forEach((b) => {
        const li = el('li', 'bm-item');
        if (adapter.isHere(b)) li.setAttribute('aria-current', 'true');
        const go = el('button', 'bm-go');
        go.type = 'button';
        go.append(el('b', null, adapter.where(b)));
        if (b.text) go.append(el('span', null, squeeze(b.text)));
        go.append(el('small', null, [b.device, timeAgo(b.created)].filter(Boolean).join(' · ')));
        go.addEventListener('click', () => {
          adapter.goTo(b);
          if (!wide.matches && adapter.closeList) adapter.closeList();
        });
        const del = el('button', 'icon-btn');
        del.type = 'button';
        del.title = t('Delete the bookmark');
        del.setAttribute('aria-label', t('Delete the bookmark'));
        const icon = el('i', 'far fa-trash-can');
        icon.setAttribute('aria-hidden', 'true');
        del.append(icon);
        del.addEventListener('click', () => removeMark(b));
        li.append(go, del);
        ul.append(li);
      });
      listBox.append(ul);
    }
    function removeMark(b) {
      const gone = marks.remove(b.id);
      if (!gone) return;
      // Undone as a new bookmark: the deletion and the undo can fall in the same second
      ReaderSync.toast(t('Bookmark removed'), { label: t('Undo'), run: () => marks.save({ ...gone, id: null, deleted: false }) });
    }
    async function toggle() {
      if (here) { removeMark(here); return; }
      const draft = await adapter.here();
      if (!draft) return;
      marks.save({ ...draft, id: null });
      ReaderSync.toast(t('Page bookmarked'));
    }
    markBtn.addEventListener('click', toggle);
    marks.onChange(() => { paintButton(); paintList(); });
    function update() { paintButton(); paintList(); }
    marks.load().then(update);
    update();
    // Bookmarks made on another device appear at once, and are fetched again after a reconnection
    document.addEventListener('readerbookmarks', (e) => marks.applyRemote(e.detail));
    document.addEventListener('readerresync', () => marks.load().then(update));

    return { update, onKey, showSearch };
  }

  window.ReaderTools = { mount, finder, folded };
})();

// Highlights and notes in the EPUB reader (epub.js), shared with KOReader and
// the Android app through Notes.store (static/app/notes.js).
//
// A highlight is saved with KOReader's xpointers of its start and end, the
// CFI of the range and the text around it. Highlights made elsewhere carry
// only xpointers: they are placed when their chapter is shown, by following
// the xpointers and, if those lead to other words, by finding the text.
(function () {
  const FILL = {
    yellow: '#f5c400', orange: '#ff9800', red: '#ef5350', purple: '#ab47bc', blue: '#42a5f5',
    cyan: '#26c6da', green: '#66bb6a', olive: '#9e9d24', gray: '#9e9e9e',
  };
  const squeeze = (s) => (s || '').replace(/\s+/g, ' ').trim();
  const strip = (s) => (s || '').replace(/\s+/g, '');

  function mount(o) {
    const { book, rendition, notes, chapterOf, percentageOf } = o;
    const bar = document.getElementById('selBar');
    const panel = document.getElementById('notesPanel');
    const notesBtn = document.getElementById('notesBtn');
    const count = document.getElementById('notesCount');
    const dialog = document.getElementById('noteDialog');
    const placed = new Map(); // id -> { cfi, look }
    let selection = null;

    // ------------------------------------------------------------ placing

    const spineOf = (a) => {
      if (a.cfi) {
        try { return new ePub.CFI(a.cfi).spinePos; } catch { }
      }
      return XPointer.spineIndex(a.pos0);
    };

    // The DOM range of a highlight in a chapter's document, or null
    function rangeIn(doc, a) {
      const range = doc.createRange();
      if (a.cfi) {
        try {
          const r = new ePub.CFI(a.cfi).toRange(doc);
          if (r && !r.collapsed) return r;
        } catch { }
      }
      let start = XPointer.resolve(a.pos0, doc);
      let end = XPointer.resolve(a.pos1, doc);
      const want = strip(a.text);
      const fits = () => {
        if (!start || !end) return false;
        try {
          range.setStart(start.node, start.offset);
          range.setEnd(end.node, end.offset);
        } catch { return false; }
        if (range.collapsed) return false;
        const got = strip(range.toString());
        return !want || got === want || (got.startsWith(want.slice(0, 16)) && Math.abs(got.length - want.length) < 4);
      };
      if (fits()) return range;
      // Find the words again, near where the xpointer led
      const ctx = a.context || {};
      start = XPointer.find(doc, { before: ctx.before || '', after: a.text }, start) || XPointer.find(doc, { after: a.text }, start);
      if (!start) return null;
      end = null;
      const chars = want.length;
      const walker = doc.createTreeWalker(doc.body, 4);
      walker.currentNode = start.node;
      let seen = 0;
      for (let t = start.node.nodeType === 3 ? start.node : walker.nextNode(); t && !end; t = walker.nextNode()) {
        for (let i = t === start.node ? start.offset : 0; i < t.data.length; i++) {
          if (!/\s/.test(t.data[i])) seen++;
          if (seen >= chars) { end = { node: t, offset: i + 1 }; break; }
        }
      }
      return fits() ? range : null;
    }

    function look(a) {
      return [a.color, a.style, document.getElementById('reader').dataset.paper].join(':');
    }

    function draw(a, cfi) {
      const paper = document.getElementById('reader').dataset.paper;
      const fill = FILL[a.color] || FILL.yellow;
      const onClick = (e) => { if (e && e.preventDefault) e.preventDefault(); edit(notes.get(a.id)); };
      const cls = 'hl-mark';
      if (a.style === 'underscore' || a.style === 'strikeout') {
        rendition.annotations.underline(cfi, { id: a.id }, onClick, cls, { stroke: fill, 'stroke-opacity': 0.9, 'stroke-width': 2 });
      } else {
        rendition.annotations.highlight(cfi, { id: a.id }, onClick, cls, {
          fill, 'fill-opacity': paper === 'dark' ? 0.35 : 0.4, 'mix-blend-mode': paper === 'dark' ? 'normal' : 'multiply',
        });
      }
      placed.set(a.id, { cfi, look: look(a), type: a.style === 'underscore' || a.style === 'strikeout' ? 'underline' : 'highlight' });
    }

    function unplace(id) {
      const p = placed.get(id);
      if (!p) return;
      try { rendition.annotations.remove(p.cfi, p.type); } catch { }
      placed.delete(id);
    }

    // Documents of the chapters on screen now, by spine index
    function shown() {
      const map = new Map();
      (rendition.views ? rendition.views() : []).forEach((v) => {
        if (v && v.contents && v.section) map.set(v.section.index, v.contents);
      });
      return map;
    }

    function cfiFor(a, contents) {
      const range = rangeIn(contents.document, a);
      return range ? contents.cfiFromRange(range) : null;
    }

    // Brings what is drawn in line with the list of highlights
    function refresh() {
      const list = notes.list();
      const ids = new Set(list.map((a) => a.id));
      Array.from(placed.keys()).forEach((id) => { if (!ids.has(id)) unplace(id); });
      const docs = shown();
      list.forEach((a) => {
        const p = placed.get(a.id);
        if (p && p.look === look(a)) return;
        let cfi = p ? p.cfi : a.cfi;
        const contents = docs.get(spineOf(a));
        if (!cfi && contents) cfi = cfiFor(a, contents);
        if (!cfi) return; // placed when its chapter is shown
        unplace(a.id);
        draw(a, cfi);
      });
      const n = list.length;
      count.textContent = n ? String(n) : '';
      count.hidden = !n;
    }

    rendition.on('rendered', (section, view) => {
      const contents = view && view.contents;
      if (!contents) return;
      notes.list().forEach((a) => {
        if (placed.has(a.id) || spineOf(a) !== section.index) return;
        const cfi = a.cfi || cfiFor(a, contents);
        if (cfi) draw(a, cfi);
      });
    });
    document.addEventListener('paperchange', () => setTimeout(refresh, 0));
    notes.onChange(() => refresh());

    // Where a highlight is, as a CFI the reader can open
    async function locate(a) {
      const p = placed.get(a.id);
      if (p) return p.cfi;
      if (a.cfi) return a.cfi;
      const section = book.spine.get(spineOf(a));
      if (!section) return null;
      await section.load(book.load.bind(book));
      const range = rangeIn(section.document, a);
      if (range) return section.cfiFromRange(range);
      if (a.percentage) return book.locations.cfiFromPercentage(a.percentage);
      return section.href;
    }

    async function goTo(a) {
      const cfi = await locate(a);
      if (!cfi) return;
      if (o.onJump) o.onJump();
      await rendition.display(cfi);
      setTimeout(refresh, 50);
    }

    // ------------------------------------------------------------ selecting

    function describe(cfiRange, contents) {
      const range = contents.range(cfiRange);
      const index = contents.sectionIndex;
      const doc = contents.document;
      const section = book.spine.get(index);
      const before = XPointer.contextAt(doc, range.startContainer, range.startOffset).before;
      const after = XPointer.contextAt(doc, range.endContainer, range.endOffset).after;
      return {
        text: squeeze(range.toString()).slice(0, 8000),
        pos0: XPointer.fromNode(range.startContainer, range.startOffset, index) || '',
        pos1: XPointer.fromNode(range.endContainer, range.endOffset, index) || '',
        cfi: cfiRange,
        context: { before, after },
        chapter: chapterOf(section && section.href) || '',
        percentage: percentageOf(cfiRange, index),
      };
    }

    function clearSelection() {
      if (selection) {
        try { selection.contents.window.getSelection().removeAllRanges(); } catch { }
      }
      selection = null;
      bar.hidden = true;
    }

    function highlight(color, note) {
      if (!selection) return null;
      const a = notes.save({ ...describe(selection.cfiRange, selection.contents), color, note: note || '' });
      clearSelection();
      return a;
    }

    const colors = Notes.swatches(storeGet('opdshelf:hl-color') || 'yellow', (c) => {
      storeSet('opdshelf:hl-color', c);
      highlight(c);
    });
    bar.prepend(colors);
    document.getElementById('selNote').onclick = () => {
      if (!selection) return;
      const draft = describe(selection.cfiRange, selection.contents);
      const sel = selection;
      edit({ ...draft, color: storeGet('opdshelf:hl-color') || 'yellow', note: '' }, true, () => { selection = sel; });
    };
    document.getElementById('selCopy').onclick = async () => {
      if (!selection) return;
      const text = squeeze(selection.contents.range(selection.cfiRange).toString());
      try { await navigator.clipboard.writeText(text); ReaderSync.toast(t('Copied')); } catch { }
      clearSelection();
    };

    rendition.on('selected', (cfiRange, contents) => {
      selection = { cfiRange, contents };
      colors.querySelectorAll('button').forEach((b) => b.setAttribute('aria-pressed', b.dataset.color === (storeGet('opdshelf:hl-color') || 'yellow')));
      bar.hidden = false;
    });
    rendition.on('click', () => {
      if (!selection) return;
      setTimeout(() => {
        const s = selection && selection.contents.window.getSelection();
        if (!s || s.isCollapsed) { selection = null; bar.hidden = true; }
      }, 300);
    });
    rendition.on('relocated', () => { if (!dialog.open) clearSelection(); });

    // ------------------------------------------------------------ note dialog

    const quote = document.getElementById('noteQuote');
    const text = document.getElementById('noteText');
    const del = document.getElementById('noteDelete');
    let editing = null;
    let editColor = 'yellow';
    let onSaveDraft = null;
    let dialogColors = null;
    function edit(a, isNew, beforeSave) {
      if (!a) return;
      editing = { ...a, isNew: Boolean(isNew) };
      onSaveDraft = beforeSave || null;
      editColor = a.color || 'yellow';
      quote.textContent = squeeze(a.text);
      text.value = a.note || '';
      del.hidden = Boolean(isNew);
      const picks = Notes.PICK.includes(editColor) ? Notes.PICK : Notes.PICK.concat(editColor);
      const fresh = Notes.swatches(editColor, (c) => {
        editColor = c;
        fresh.querySelectorAll('button').forEach((b) => b.setAttribute('aria-pressed', b.dataset.color === c));
      }, picks);
      if (dialogColors) dialogColors.replaceWith(fresh); else document.getElementById('noteColors').append(fresh);
      dialogColors = fresh;
      document.getElementById('noteTitle').textContent = isNew ? t('New note') : (a.note ? t('Edit note') : t('Highlight'));
      dialog.showModal();
      setTimeout(() => text.focus(), 0);
    }
    document.getElementById('noteForm').addEventListener('submit', (e) => {
      e.preventDefault();
      if (!editing) return;
      const { isNew, ...a } = editing;
      if (isNew) {
        if (onSaveDraft) onSaveDraft();
        highlight(editColor, text.value.trim());
      } else {
        notes.save({ ...a, color: editColor, note: text.value.trim() });
      }
      editing = null;
      dialog.close();
    });
    text.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) document.getElementById('noteForm').requestSubmit();
    });
    document.getElementById('noteCancel').onclick = () => { editing = null; dialog.close(); };
    del.onclick = () => {
      if (!editing) return;
      const gone = notes.remove(editing.id);
      editing = null;
      dialog.close();
      if (gone) ReaderSync.toast(t('Highlight deleted'), { label: t('Undo'), run: () => notes.restore(gone) });
    };

    // ------------------------------------------------------------ side panel

    const wide = window.matchMedia('(min-width: 761px)');
    const list = Notes.view(document.getElementById('notesList'), notes, {
      compact: true,
      open: (a) => { if (!wide.matches) showPanel(false); goTo(a); },
      toast: (message, action) => ReaderSync.toast(message, action),
    });
    function showPanel(open) {
      panel.hidden = !open;
      notesBtn.setAttribute('aria-pressed', open);
      if (open) list.paint();
      requestAnimationFrame(() => { if (rendition.manager) rendition.resize(); });
    }
    notesBtn.onclick = () => showPanel(panel.hidden);
    document.getElementById('notesClose').onclick = () => showPanel(false);
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && !panel.hidden && !dialog.open) showPanel(false);
    });

    refresh();
    return {
      refresh,
      goTo,
      selection: () => selection,
      clearSelection,
      openFromHash() {
        const m = /(?:^|[#&])note=([\w-]+)/.exec(location.hash);
        if (!m) return false;
        const a = notes.get(m[1]);
        history.replaceState(history.state, '', location.pathname + location.search);
        if (!a || a.deleted) return false;
        goTo(a);
        return true;
      },
    };
  }

  window.ReaderHighlights = { mount };
})();

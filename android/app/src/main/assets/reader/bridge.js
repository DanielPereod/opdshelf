// Runs inside Readium's web view, after xpointer.js. Turns what is on screen
// into KOReader xpointers and back, like the web reader does.
(function () {
  if (window.OPDShelf) return;
  const MARK = 'opdshelf-mark';

  // First text position at or after a point of the screen
  function caretAt(x, y) {
    if (document.caretRangeFromPoint) {
      const r = document.caretRangeFromPoint(x, y);
      return r && { node: r.startContainer, offset: r.startOffset };
    }
    if (document.caretPositionFromPoint) {
      const p = document.caretPositionFromPoint(x, y);
      return p && { node: p.offsetNode, offset: p.offset };
    }
    return null;
  }

  function visible(point) {
    const range = document.createRange();
    range.setStart(point.node, point.offset);
    range.setEnd(point.node, point.offset);
    const rect = range.getBoundingClientRect();
    return rect.right >= 0 && rect.left < window.innerWidth && rect.bottom >= 0 && rect.top < window.innerHeight;
  }

  function describe(point, spine) {
    const progress = XPointer.fromNode(point.node, point.offset, spine);
    if (!progress) return null;
    return { progress, context: XPointer.contextAt(document, point.node, point.offset) };
  }

  // The first text visible on screen
  function firstVisible(spine) {
    const w = window.innerWidth;
    const h = window.innerHeight;
    for (let y = 8; y < h; y += 16) {
      for (let x = 8; x < w; x += Math.max(24, w / 8)) {
        const point = caretAt(x, y);
        if (point && point.node.nodeType === 3 && point.node.data.trim() && visible(point)) {
          return describe(point, spine);
        }
      }
    }
    return describe({ node: document.body, offset: 0 }, spine);
  }

  // The start of the selected text
  function selection(spine) {
    const s = window.getSelection();
    if (!s || s.rangeCount === 0) return null;
    const range = s.getRangeAt(0);
    const result = describe({ node: range.startContainer, offset: range.startOffset }, spine);
    if (!result) return null;
    result.context.selected = s.toString().replace(/\s+/g, ' ').trim().slice(0, 200);
    return result;
  }

  // The selected range as a highlight: xpointers of its start and end, its text and the words around it
  function selectionRange(spine) {
    const s = window.getSelection();
    if (!s || s.rangeCount === 0) return null;
    const range = s.getRangeAt(0);
    if (range.collapsed) return null;
    const pos0 = XPointer.fromNode(range.startContainer, range.startOffset, spine);
    const pos1 = XPointer.fromNode(range.endContainer, range.endOffset, spine);
    if (!pos0 || !pos1) return null;
    return {
      pos0,
      pos1,
      text: s.toString().replace(/\s+/g, ' ').trim().slice(0, 8000),
      context: {
        before: XPointer.contextAt(document, range.startContainer, range.startOffset).before || '',
        after: XPointer.contextAt(document, range.endContainer, range.endOffset).after || '',
      },
    };
  }

  // Where an xpointer is, falling back to the saved text around it
  function locate(xpointer, context) {
    let point = XPointer.resolve(xpointer, document);
    if (point && context && !XPointer.matches(document, point, context)) {
      point = XPointer.find(document, context, point) || point;
    }
    if (!point) point = XPointer.find(document, context);
    return point;
  }

  function rectOf(point) {
    const range = document.createRange();
    range.setStart(point.node, point.offset);
    range.setEnd(point.node, point.offset);
    let rect = range.getBoundingClientRect();
    if (!rect.width && !rect.height && point.node.nodeType === 3) {
      range.selectNodeContents(point.node.parentNode);
      rect = range.getBoundingClientRect();
    } else if (!rect.width && !rect.height && point.node.nodeType === 1) {
      rect = point.node.getBoundingClientRect();
    }
    return rect;
  }

  // Progression in the chapter (0 to 1) to show the xpointer, as Readium expects it
  function progression(xpointer, context, scroll) {
    const point = locate(xpointer, context);
    if (!point) return -1;
    const rect = rectOf(point);
    const root = document.scrollingElement || document.documentElement;
    if (scroll) {
      return Math.max(0, Math.min(1, (rect.top + root.scrollTop - 16) / root.scrollHeight));
    }
    return Math.max(0, Math.min(1, (rect.left + root.scrollLeft + 1) / root.scrollWidth));
  }

  // Highlights the passage starting at the xpointer
  function highlight(xpointer, context) {
    if (!window.CSS || !CSS.highlights || typeof Highlight === 'undefined') return false;
    const point = locate(xpointer, context);
    if (!point) return false;
    if (!document.getElementById(MARK)) {
      const style = document.createElement('style');
      style.id = MARK;
      style.textContent = `::highlight(${MARK}) { background-color: rgba(255, 196, 0, 0.45); }`;
      document.head.append(style);
    }
    const wanted = Math.max(((context && context.selected) || '').replace(/\s+/g, '').length, 12);
    const walker = document.createTreeWalker(document.body, 4);
    const range = document.createRange();
    range.setStart(point.node, point.offset);
    let count = 0;
    let end = null;
    walker.currentNode = point.node;
    for (let t = point.node.nodeType === 3 ? point.node : walker.nextNode(); t && !end; t = walker.nextNode()) {
      for (let i = t === point.node ? point.offset : 0; i < t.data.length; i++) {
        if (!/\s/.test(t.data[i])) count++;
        if (count >= wanted && (i + 1 >= t.data.length || /\s/.test(t.data[i + 1]))) {
          end = { node: t, offset: i + 1 };
          break;
        }
      }
      if (!end) range.setEnd(t, t.data.length);
    }
    if (end) range.setEnd(end.node, end.offset);
    CSS.highlights.set(MARK, new Highlight(range));
    setTimeout(() => CSS.highlights.delete(MARK), 6000);
    return true;
  }

  // Android's WebView scrolls the page sideways while a selection handle is dragged in
  // paginated (multi-column) books, so the selection jumps to another page and grabs text
  // the reader never meant (readium/kotlin-toolkit#325, a Chromium bug). While the selection
  // is changing, the page is kept where it was; programmatic page turns later are not affected.
  (function keepPageWhileSelecting() {
    const root = () => document.scrollingElement || document.documentElement;
    let anchor = null;
    let changed = 0;
    document.addEventListener('selectionchange', () => {
      const s = window.getSelection();
      if (!s || s.isCollapsed) {
        anchor = null;
        return;
      }
      if (anchor === null) anchor = { left: root().scrollLeft, top: root().scrollTop };
      changed = Date.now();
    });
    window.addEventListener('scroll', () => {
      if (anchor === null || Date.now() - changed > 400) {
        if (anchor !== null) anchor = { left: root().scrollLeft, top: root().scrollTop };
        return;
      }
      const el = root();
      // Only sideways: scrolled books move up and down on purpose
      if (el.scrollWidth > window.innerWidth + 1 && el.scrollLeft !== anchor.left) el.scrollLeft = anchor.left;
    }, true);
  })();

  window.OPDShelf = { firstVisible, selection, selectionRange, progression, highlight };
})();

// Converts positions in an EPUB chapter to and from KOReader's xpointers, e.g.
// /body/DocFragment[3]/body/div[1]/p[12]/text()[1].45
//
// DocFragment[N] is the Nth item of the spine. Below it the path follows the
// chapter's own HTML. KOReader (crengine) parses the HTML slightly differently
// from a browser, and this mirrors what matters for positions:
// - whitespace-only text at the very start of a block element is dropped;
// - offsets count characters after collapsing runs of whitespace to one space.
// Any mismatch left is caught by the text around the position, which is saved
// with it and searched for when the path does not lead to the same words.
(function () {
  const INLINE = new Set(('a abbr acronym b bdi bdo big br cite code del dfn em font i img input ins kbd label mark ' +
    'q rp rt ruby s samp small span strike strong sub sup time tt u var wbr').split(' '));
  const CONTEXT = 40;

  const isSpace = (c) => c === ' ' || c === '\n' || c === '\r' || c === '\t';
  const nameOf = (el) => (el.localName || el.nodeName || '').toLowerCase();
  const bodyOf = (doc) => doc.body || doc.getElementsByTagName('body')[0] || null;

  // Whether crengine keeps this text node
  function counted(text) {
    if (!/^[ \t\n\r]*$/.test(text.data)) return true;
    const parent = text.parentNode;
    if (!parent || parent.nodeType !== 1 || INLINE.has(nameOf(parent))) return true;
    for (let n = text.previousSibling; n; n = n.previousSibling) {
      if (n.nodeType === 1 || n.nodeType === 3) return true;
    }
    return false; // first whitespace of a block
  }

  const texts = (el) => Array.from(el.childNodes).filter((n) => n.nodeType === 3 && counted(n));
  const elements = (el, name) => Array.from(el.children || []).filter((c) => nameOf(c) === name);

  // Offset in the collapsed text (in characters) of a raw offset (in UTF-16 units)
  function collapsedOffset(s, raw) {
    let n = 0;
    let space = false;
    for (let i = 0; i < raw && i < s.length;) {
      const code = s.codePointAt(i);
      const sp = isSpace(s[i]);
      if (!(sp && space)) n++;
      space = sp;
      i += code > 0xffff ? 2 : 1;
    }
    return n;
  }

  function rawOffset(s, collapsed) {
    let n = 0;
    let space = false;
    for (let i = 0; i < s.length;) {
      const code = s.codePointAt(i);
      const sp = isSpace(s[i]);
      if (!(sp && space)) {
        if (n === collapsed) return i;
        n++;
      }
      space = sp;
      i += code > 0xffff ? 2 : 1;
    }
    return s.length;
  }

  // xpointer of a DOM position (a text node and offset, or an element and child index)
  function fromNode(node, offset, spineIndex) {
    let tail = '';
    let n = node;
    if (n.nodeType === 3) {
      const index = texts(n.parentNode).indexOf(n);
      if (index >= 0) tail = `/text()[${index + 1}].${collapsedOffset(n.data, offset)}`;
      n = n.parentNode;
    } else if (n.nodeType === 1 && offset < n.childNodes.length) {
      const child = n.childNodes[offset];
      if (child.nodeType === 1) n = child;
      else if (child.nodeType === 3 && counted(child)) tail = `/text()[${texts(n).indexOf(child) + 1}].0`;
    }
    while (n && n.nodeType === 1 && nameOf(n) !== 'body') {
      const name = nameOf(n);
      tail = `/${name}[${elements(n.parentNode, name).indexOf(n) + 1}]` + tail;
      n = n.parentNode;
    }
    if (!n || n.nodeType !== 1) return null;
    return `/body/DocFragment[${spineIndex + 1}]/body${tail}`;
  }

  // Spine index (from 0) an xpointer points into, or -1
  function spineIndex(xpointer) {
    const m = /^\/body\/DocFragment(?:\[(\d+)\])?(?:\/|$)/.exec(xpointer || '');
    return m ? Number(m[1] || 1) - 1 : -1;
  }

  // DOM position of an xpointer inside the chapter's document, or null
  function resolve(xpointer, doc) {
    const m = /^\/body\/DocFragment(?:\[\d+\])?\/body((?:\/[^/]+)*)$/.exec(xpointer || '');
    let n = bodyOf(doc);
    if (!m || !n) return null;
    const steps = m[1].split('/').filter(Boolean);
    for (let k = 0; k < steps.length; k++) {
      let step = steps[k];
      let offset = null;
      const dot = /^(.*)\.(\d+)$/.exec(step);
      if (dot && k === steps.length - 1) {
        step = dot[1];
        offset = Number(dot[2]);
      }
      const s = /^([^[\]]+)(?:\[(\d+)\])?$/.exec(step);
      if (!s) return null;
      const name = s[1].toLowerCase();
      const index = s[2] ? Number(s[2]) : 1;
      if (name === 'text()') {
        const text = texts(n)[index - 1];
        return text ? { node: text, offset: rawOffset(text.data, offset || 0) } : null;
      }
      const next = elements(n, name)[index - 1];
      if (!next) return null;
      n = next;
      if (offset !== null) return { node: n, offset: Math.min(offset, n.childNodes.length) };
    }
    return { node: n, offset: 0 };
  }

  const strip = (s) => (s || '').replace(/\s+/g, '');
  const squeeze = (s) => s.replace(/\s+/g, ' ');

  // Text before and after a position, to find it again later
  function contextAt(doc, node, offset) {
    const body = bodyOf(doc);
    if (!body) return {};
    const range = doc.createRange();
    range.setStart(body, 0);
    range.setEnd(node, offset);
    const before = squeeze(range.toString()).slice(-CONTEXT);
    range.setStart(node, offset);
    range.setEnd(body, body.childNodes.length);
    const after = squeeze(range.toString()).slice(0, CONTEXT);
    return { before, after };
  }

  // Every non-space character of the chapter, with where it is
  function characters(doc) {
    const body = bodyOf(doc);
    const chars = [];
    const where = [];
    if (!body) return { text: '', where };
    const walker = doc.createTreeWalker(body, 4 /* NodeFilter.SHOW_TEXT */);
    for (let t = walker.nextNode(); t; t = walker.nextNode()) {
      const s = t.data;
      for (let i = 0; i < s.length; i++) {
        if (/\s/.test(s[i])) continue;
        chars.push(s[i]);
        where.push([t, i]);
      }
    }
    return { text: chars.join(''), where };
  }

  // Finds the position the saved text describes, closest to `near` if it appears more than once
  function find(doc, context, near) {
    if (!context) return null;
    const before = strip(context.before).slice(-24);
    const after = strip(context.after || context.selected).slice(0, 24);
    if (!before && !after) return null;
    const { text, where } = characters(doc);
    let nearIndex = 0;
    if (near) {
      const i = where.findIndex(([t, o]) => {
        if (t === near.node) return o >= near.offset;
        return Boolean(near.node.compareDocumentPosition(t) & 4 /* FOLLOWING */);
      });
      nearIndex = i < 0 ? text.length : i;
    }
    const search = (needle, shift) => {
      if (needle.length < 6) return -1;
      let best = -1;
      for (let i = text.indexOf(needle); i >= 0; i = text.indexOf(needle, i + 1)) {
        if (best < 0 || Math.abs(i + shift - nearIndex) < Math.abs(best - nearIndex)) best = i + shift;
      }
      return best;
    };
    let at = search(before + after, before.length);
    if (at < 0) at = search(after, 0);
    if (at < 0) at = search(before, before.length);
    if (at < 0) return null;
    if (at >= where.length) {
      const last = where[where.length - 1];
      return last ? { node: last[0], offset: last[1] + 1 } : null;
    }
    return { node: where[at][0], offset: where[at][1] };
  }

  // Whether the saved text follows the position, i.e. the xpointer led to the right words
  function matches(doc, point, context) {
    const expected = strip(context && context.after).slice(0, 12);
    if (!expected) return true;
    return strip(contextAt(doc, point.node, point.offset).after).startsWith(expected.slice(0, 8));
  }

  window.XPointer = { fromNode, resolve, spineIndex, contextAt, find, matches };
})();

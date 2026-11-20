// The panel's own menu of actions, for books and folders: opened at the
// pointer with a right click, or below a "⋯" button or the focused item with
// the keyboard (the menu key or Shift+F10).
//
//   Menu.open(items, { x, y } or { anchor }, { trigger, label, header })
//
// Each item is { label, icon, run } or { label, icon, href, download }, with
// `danger` for actions that delete and `disabled` plus `hint` for an action
// that cannot be done now; '-' is a separator. Arrow keys, Home and End move
// between items, Enter or Space runs one, Esc closes the menu and gives the
// focus back to `trigger`. A click elsewhere, scrolling or resizing closes it.
var Menu = (function () {
  let current = null;
  // A click on the button that opened the menu closes it instead of opening it again
  let lastClosed = { trigger: null, at: 0 };

  function items() {
    return current ? Array.from(current.el.querySelectorAll('[role="menuitem"]')) : [];
  }

  function focusItem(index) {
    const list = items();
    if (!list.length) return;
    const item = list[(index + list.length) % list.length];
    item.focus();
  }

  function close(returnFocus) {
    if (!current) return;
    const { el, trigger } = current;
    current = null;
    el.remove();
    if (trigger && trigger.getAttribute('aria-haspopup')) trigger.setAttribute('aria-expanded', 'false');
    document.removeEventListener('pointerdown', onPointerDown, true);
    document.removeEventListener('scroll', onScroll, true);
    window.removeEventListener('resize', onResize);
    window.removeEventListener('blur', onResize);
    if (returnFocus && trigger && document.contains(trigger)) trigger.focus({ preventScroll: true });
  }

  function onPointerDown(e) {
    if (!current || current.el.contains(e.target)) return;
    const { trigger } = current;
    // Pressing the button that opened the menu closes it, and its click must not open it again
    if (trigger && trigger.getAttribute('aria-haspopup') && trigger.contains(e.target)) lastClosed = { trigger, at: Date.now() };
    close(false);
  }
  function onScroll(e) {
    if (!current || (e.target instanceof Node && current.el.contains(e.target))) return;
    // A scroll that was already on its way when the menu opened (the item
    // brought into view) does not close it
    if (Date.now() - current.at < 150) return;
    close(false);
  }
  function onResize() {
    close(false);
  }

  function activate(item) {
    if (!item || item.getAttribute('aria-disabled') === 'true') return;
    const run = item._run;
    // The focus goes back first, so a dialog the action opens returns it there
    close(true);
    if (run) run();
  }

  function onKeyDown(e) {
    const list = items();
    const at = list.indexOf(document.activeElement);
    switch (e.key) {
      case 'ArrowDown': focusItem(at + 1); break;
      case 'ArrowUp': focusItem(at < 0 ? -1 : at - 1); break;
      case 'Home': focusItem(0); break;
      case 'End': focusItem(-1); break;
      case 'Escape': close(true); break;
      case 'Tab': close(true); break;
      case 'Enter':
      case ' ': {
        const item = list[at];
        if (!item) break;
        if (item.tagName === 'A' && item.getAttribute('aria-disabled') !== 'true') {
          // A link opens as a link: in a new tab with a modifier, as a download…
          close(true);
          item.click();
        } else activate(item);
        break;
      }
      default: {
        // Typing a letter goes to the next item that starts with it
        if (e.key.length !== 1 || e.ctrlKey || e.metaKey || e.altKey) return;
        const letter = e.key.toLocaleLowerCase();
        const next = list.slice(at + 1).concat(list.slice(0, at + 1))
          .find((i) => i.textContent.trim().toLocaleLowerCase().startsWith(letter));
        if (next) next.focus();
        else return;
      }
    }
    e.preventDefault();
    e.stopPropagation();
  }

  function build(entries, options) {
    const el = document.createElement('div');
    el.className = 'ctx-menu';
    el.setAttribute('role', 'menu');
    el.setAttribute('aria-label', options.label || options.header || '');
    el.tabIndex = -1;
    if (options.header) {
      const head = document.createElement('div');
      head.className = 'ctx-head';
      head.setAttribute('role', 'presentation');
      head.textContent = options.header;
      el.append(head);
    }
    let lastWasSeparator = true;
    entries.forEach((entry) => {
      if (!entry) return;
      if (entry === '-') {
        if (lastWasSeparator) return;
        const sep = document.createElement('div');
        sep.className = 'ctx-sep';
        sep.setAttribute('role', 'separator');
        el.append(sep);
        lastWasSeparator = true;
        return;
      }
      lastWasSeparator = false;
      const item = document.createElement(entry.href && !entry.disabled ? 'a' : 'button');
      item.className = 'ctx-item' + (entry.danger ? ' danger' : '');
      item.setAttribute('role', 'menuitem');
      item.tabIndex = -1;
      if (item.tagName === 'A') {
        item.href = entry.href;
        if (entry.download) item.setAttribute('download', '');
        item.addEventListener('click', () => setTimeout(() => close(false)));
      } else {
        item.type = 'button';
        item._run = entry.run;
        item.addEventListener('click', () => activate(item));
      }
      if (entry.disabled) {
        item.setAttribute('aria-disabled', 'true');
        if (entry.hint) item.title = entry.hint;
      }
      const icon = document.createElement('i');
      icon.className = entry.icon || '';
      icon.setAttribute('aria-hidden', 'true');
      const text = document.createElement('span');
      text.className = 'ctx-label';
      text.textContent = entry.label;
      item.append(icon, text);
      if (entry.disabled && entry.hint) {
        const hint = document.createElement('small');
        hint.className = 'ctx-hint';
        hint.textContent = entry.hint;
        text.append(hint);
      }
      el.append(item);
    });
    // No separator at the end
    while (el.lastElementChild && el.lastElementChild.classList.contains('ctx-sep')) el.lastElementChild.remove();
    el.addEventListener('keydown', onKeyDown);
    el.addEventListener('contextmenu', (e) => e.preventDefault());
    // The pointer moving over an item focuses it, so the keyboard carries on from there
    el.addEventListener('pointermove', (e) => {
      const item = e.target.closest('[role="menuitem"]');
      if (item && document.activeElement !== item) item.focus({ preventScroll: true });
    });
    return el;
  }

  // Places the menu at the point or under the anchor, always inside the window
  function place(el, at) {
    const margin = 8;
    const vw = document.documentElement.clientWidth;
    const vh = window.innerHeight;
    el.style.maxHeight = (vh - margin * 2) + 'px';
    const { width, height } = el.getBoundingClientRect();
    let left;
    let top;
    if (at.anchor) {
      const r = at.anchor.getBoundingClientRect();
      left = r.right - width;
      if (left < margin) left = r.left;
      top = r.bottom + 4;
      if (top + height > vh - margin) top = r.top - height - 4;
    } else {
      left = at.x;
      top = at.y;
      if (left + width > vw - margin) left = at.x - width;
      if (top + height > vh - margin) top = at.y - height;
    }
    left = Math.max(margin, Math.min(left, vw - width - margin));
    top = Math.max(margin, Math.min(top, vh - height - margin));
    el.style.left = Math.round(left) + 'px';
    el.style.top = Math.round(top) + 'px';
  }

  function open(entries, at, options) {
    options = options || {};
    const trigger = options.trigger || document.activeElement;
    if (lastClosed.trigger && lastClosed.trigger === options.trigger && Date.now() - lastClosed.at < 600) {
      lastClosed = { trigger: null, at: 0 };
      return;
    }
    close(false);
    const el = build(entries, options);
    el.style.visibility = 'hidden';
    document.body.append(el);
    place(el, at);
    el.style.visibility = '';
    current = { el, trigger, at: Date.now() };
    if (trigger && trigger.getAttribute('aria-haspopup')) trigger.setAttribute('aria-expanded', 'true');
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', onResize);
    window.addEventListener('blur', onResize);
    const first = items().find((i) => i.getAttribute('aria-disabled') !== 'true') || items()[0];
    if (first) first.focus({ preventScroll: true });
    else el.focus();
  }

  return { open, close, isOpen: () => Boolean(current) };
})();

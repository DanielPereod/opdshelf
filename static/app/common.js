// Helpers shared by every page of the panel and the reader: texts in the
// page's language, relative times and device icons.
// Texts in the page's language (see src/i18n.rs)
var I18N = JSON.parse(document.currentScript.dataset.i18n || '{}');
function t(text, values) {
  let out = I18N[text] || text;
  if (values) Object.entries(values).forEach(([k, v]) => { out = out.split('{' + k + '}').join(v); });
  return out;
}

// "5 minutes ago", in the page's language
function timeAgo(seconds) {
  if (!seconds) return '';
  const lang = document.documentElement.lang || undefined;
  const diff = seconds - (Date.now() / 1000 + (window.serverOffset || 0));
  const abs = Math.abs(diff);
  const rtf = new Intl.RelativeTimeFormat(lang, { numeric: 'auto' });
  if (abs < 45) return t('just now');
  const units = [['minute', 60, 3600], ['hour', 3600, 86400], ['day', 86400, 604800], ['week', 604800, 2629800],
    ['month', 2629800, 31557600], ['year', 31557600, Infinity]];
  for (const [unit, size, limit] of units) {
    if (abs < limit) return rtf.format(Math.round(diff / size), unit);
  }
  return '';
}

// "4,7 MB", with the decimal separator of the page's language
function formatBytes(bytes) {
  const lang = document.documentElement.lang || undefined;
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = Number(bytes) || 0;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return new Intl.NumberFormat(lang, { maximumFractionDigits: digits, minimumFractionDigits: digits }).format(value) + ' ' + units[unit];
}

// "English" or "Inglés" for "en", in the page's language; the code itself
// when the browser does not know it
function languageName(code) {
  const lang = document.documentElement.lang || 'en';
  try {
    const name = new Intl.DisplayNames([lang], { type: 'language' }).of(code.trim());
    if (name && name.toLowerCase() !== code.trim().toLowerCase()) return name.charAt(0).toLocaleUpperCase(lang) + name.slice(1);
  } catch {
    // Not a language code, such as "English" written out
  }
  return code;
}

// Values the server writes as they are, shown in the page's language:
// sizes (data-bytes), days (data-day) and language codes (data-lang-code)
function localizeFields(root) {
  const lang = document.documentElement.lang || undefined;
  const day = new Intl.DateTimeFormat(lang, { day: 'numeric', month: 'short', year: 'numeric' });
  root.querySelectorAll('[data-bytes]').forEach((el) => { el.textContent = formatBytes(el.dataset.bytes); });
  root.querySelectorAll('[data-day]').forEach((el) => {
    const date = new Date(el.dataset.day);
    if (isNaN(date)) return;
    el.textContent = day.format(date);
    el.title = date.toLocaleString(lang);
  });
  root.querySelectorAll('[data-lang-code]').forEach((el) => {
    el.textContent = languageName(el.dataset.langCode);
    if (el.textContent !== el.dataset.langCode) el.title = el.dataset.langCode;
  });
}

// A cover drawn from the title and author, for books without one: a muted
// colour picked from the title, the title and the author. A real cover, when
// it loads, is drawn on top of it.
function placeholderCover(thumb, title, author) {
  let hash = 0;
  for (const c of title || '') hash = (hash * 31 + c.codePointAt(0)) >>> 0;
  thumb.classList.add('ph');
  thumb.style.setProperty('--ph-hue', hash % 360);
  const box = document.createElement('span');
  box.className = 'ph-art';
  box.setAttribute('aria-hidden', 'true');
  const initial = document.createElement('b');
  initial.className = 'ph-initial';
  initial.textContent = (title || '?').trim().charAt(0).toUpperCase();
  const name = document.createElement('b');
  name.className = 'ph-title';
  name.textContent = title || '';
  const by = document.createElement('small');
  by.className = 'ph-author';
  by.textContent = author || '';
  box.append(initial, name, by);
  thumb.prepend(box);
}

// An icon for a device from its name
function deviceIcon(name) {
  const n = (name || '').toLowerCase();
  if (/chrome|firefox|safari|edge|browser|navegador/.test(n)) return 'fas fa-globe';
  if (/android|phone|pixel|galaxy|xiaomi|iphone|móvil|movil/.test(n)) return 'fas fa-mobile-screen';
  return 'fas fa-tablet-screen-button';
}

// A book path as it goes in a URL
function encodePath(path) {
  return path.split('/').map(encodeURIComponent).join('/');
}

// This browser as a device of the sync: an id kept in this browser and a name
// such as "Firefox on Linux". The reader and the panel use the same one.
function opdshelfDevice() {
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
  return { id: deviceId(), name: deviceName() };
}

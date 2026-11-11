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

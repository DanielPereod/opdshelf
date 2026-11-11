// Text settings of the EPUB and TXT readers: font, line spacing, margins and
// justification, in a panel under the "Aa" button of the top bar. The reader
// passes a function that applies them; they are kept for every book.
(function () {
  const KEY = 'opdshelf:typography';
  const DEFAULTS = { font: 'original', spacing: 'normal', margins: 'normal', justify: false };
  const FONTS = {
    original: null,
    serif: 'Georgia, "Iowan Old Style", "Times New Roman", serif',
    sans: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
  };
  const SPACING = { tight: 1.35, normal: 1.6, loose: 1.9 };
  // Share of the page width left on each side
  const MARGINS = { narrow: 0.02, normal: 0.06, wide: 0.12 };

  function load() {
    try {
      return { ...DEFAULTS, ...JSON.parse(storeGet(KEY) || '{}') };
    } catch {
      return { ...DEFAULTS };
    }
  }

  function group(label, name, options, settings, onPick) {
    const box = document.createElement('div');
    box.className = 'typo-row';
    const title = document.createElement('span');
    title.className = 'typo-label';
    title.id = 'typo-' + name;
    title.textContent = label;
    const buttons = document.createElement('div');
    buttons.className = 'typo-options';
    buttons.setAttribute('role', 'group');
    buttons.setAttribute('aria-labelledby', title.id);
    options.forEach(([value, text, style]) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.textContent = text;
      if (style) b.style.cssText = style;
      b.setAttribute('aria-pressed', settings[name] === value);
      b.addEventListener('click', () => {
        buttons.querySelectorAll('button').forEach((o) => o.setAttribute('aria-pressed', o === b));
        onPick(value);
      });
      buttons.append(b);
    });
    box.append(title, buttons);
    return box;
  }

  function mount(apply) {
    const controls = document.getElementById('controls');
    const settings = load();
    const save = () => { storeSet(KEY, JSON.stringify(settings)); apply(resolve(settings)); };

    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'icon-btn typo-btn';
    button.title = t('Text settings');
    button.setAttribute('aria-label', t('Text settings'));
    button.setAttribute('aria-expanded', 'false');
    button.setAttribute('aria-controls', 'typoPanel');
    button.innerHTML = '<span aria-hidden="true">Aa</span>';
    controls.append(button);

    const panel = document.createElement('div');
    panel.className = 'typo-panel';
    panel.id = 'typoPanel';
    panel.setAttribute('role', 'dialog');
    panel.setAttribute('aria-label', t('Text settings'));
    panel.hidden = true;

    const size = document.createElement('div');
    size.className = 'typo-row';
    size.innerHTML = '<span class="typo-label"></span><div class="typo-options"><button type="button" data-step="-1"></button><button type="button" data-step="1"></button></div>';
    size.querySelector('.typo-label').textContent = t('Text size');
    const [smaller, larger] = size.querySelectorAll('button');
    smaller.innerHTML = '<span aria-hidden="true" style="font-size:12px">A</span>';
    smaller.setAttribute('aria-label', t('Smaller text'));
    larger.innerHTML = '<span aria-hidden="true" style="font-size:18px">A</span>';
    larger.setAttribute('aria-label', t('Larger text'));
    smaller.addEventListener('click', () => document.getElementById('smaller').click());
    larger.addEventListener('click', () => document.getElementById('larger').click());

    const justify = document.createElement('label');
    justify.className = 'typo-row typo-check';
    justify.innerHTML = '<span class="typo-label"></span><input type="checkbox">';
    justify.querySelector('.typo-label').textContent = t('Justify text');
    const check = justify.querySelector('input');
    check.checked = settings.justify;
    check.addEventListener('change', () => { settings.justify = check.checked; save(); });

    panel.append(
      size,
      group(t('Font'), 'font', [
        ['original', t('Book')],
        ['serif', t('Serif'), 'font-family:' + FONTS.serif],
        ['sans', t('Sans'), 'font-family:' + FONTS.sans],
      ], settings, (v) => { settings.font = v; save(); }),
      group(t('Line spacing'), 'spacing', [
        ['tight', t('Tight')], ['normal', t('Normal')], ['loose', t('Loose')],
      ], settings, (v) => { settings.spacing = v; save(); }),
      group(t('Margins'), 'margins', [
        ['narrow', t('Narrow')], ['normal', t('Normal')], ['wide', t('Wide')],
      ], settings, (v) => { settings.margins = v; save(); }),
      justify,
    );
    document.getElementById('reader').append(panel);

    const open = (show) => {
      panel.hidden = !show;
      button.setAttribute('aria-expanded', show);
      button.setAttribute('aria-pressed', show);
      if (show) panel.querySelector('button').focus();
    };
    button.addEventListener('click', () => open(panel.hidden));
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && !panel.hidden) { open(false); button.focus(); }
    });
    document.addEventListener('click', (e) => {
      if (!panel.hidden && !panel.contains(e.target) && !button.contains(e.target)) open(false);
    });

    apply(resolve(settings));
  }

  function resolve(settings) {
    return {
      fontFamily: FONTS[settings.font] || null,
      lineHeight: SPACING[settings.spacing] || SPACING.normal,
      margin: MARGINS[settings.margins] ?? MARGINS.normal,
      margins: settings.margins in MARGINS ? settings.margins : 'normal',
      justify: Boolean(settings.justify),
    };
  }

  window.ReaderTypography = { mount };
})();

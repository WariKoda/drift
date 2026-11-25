(() => {
  const STORAGE_KEY = 'drift-theme';
  const root = document.documentElement;
  const german = root.lang === 'de';
  const systemDark = window.matchMedia('(prefers-color-scheme: dark)');
  const toggle = document.querySelector('.theme-toggle');
  const lightMeta = document.querySelector('meta[name="theme-color"][media*="light"]');
  const darkMeta = document.querySelector('meta[name="theme-color"][media*="dark"]');

  function systemTheme() {
    return systemDark.matches ? 'dark' : 'light';
  }

  function activeTheme() {
    const stored = root.dataset.theme;
    return stored === 'dark' || stored === 'light' ? stored : systemTheme();
  }

  function syncUI() {
    const dark = activeTheme() === 'dark';
    const override = root.dataset.theme;
    if (lightMeta && darkMeta) {
      lightMeta.content = override === 'dark' ? '#181625' : '#f4f3f9';
      darkMeta.content = override === 'light' ? '#f4f3f9' : '#181625';
    }
    if (!toggle) return;
    const label = dark
      ? (german ? 'Zum hellen Design wechseln' : 'Switch to light mode')
      : (german ? 'Zum dunklen Design wechseln' : 'Switch to dark mode');
    toggle.setAttribute('aria-label', label);
    toggle.title = label;
  }

  if (toggle) {
    toggle.hidden = false;
    toggle.addEventListener('click', () => {
      const next = activeTheme() === 'dark' ? 'light' : 'dark';
      if (next === systemTheme()) {
        delete root.dataset.theme;
        try { localStorage.removeItem(STORAGE_KEY); } catch {}
      } else {
        root.dataset.theme = next;
        try { localStorage.setItem(STORAGE_KEY, next); } catch {}
      }
      syncUI();
    });
  }

  systemDark.addEventListener('change', syncUI);
  syncUI();
})();

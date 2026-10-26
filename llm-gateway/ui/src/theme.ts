/* Theme runtime: manifests come from GET /api/themes; a theme's files are
   served from /themes/{id}/... (built-in embedded, or external themes/ dir
   next to the config). Applied by injecting/removing <link>/<script> nodes.
   The boot injector in ui/index.html re-applies the cached manifest before
   first paint so there's no flash of the default theme. */

export interface ThemeMeta {
  id: string;
  name: string;
  author: string;
  version: string;
  description: string;
  mode: 'light' | 'dark';
  css: string[];
  js: string[];
  preview: string;
}

export const DEFAULT_THEME_ID = 'sakura-dark';
const CACHE_KEY = 'sakura-theme-manifest';
const lastKey = (mode: string) => `sakura-last-${mode}`;

export function cachedManifest(): ThemeMeta | null {
  try {
    const m = JSON.parse(localStorage.getItem(CACHE_KEY) || 'null');
    return m && m.id ? m : null;
  } catch (_) { return null; }
}

/** Last-used theme id per mode — the topbar sun/moon button toggles between them. */
export function lastThemeOfMode(mode: 'light' | 'dark'): string {
  try {
    return localStorage.getItem(lastKey(mode)) || (mode === 'light' ? 'sakura-light' : DEFAULT_THEME_ID);
  } catch (_) { return mode === 'light' ? 'sakura-light' : DEFAULT_THEME_ID; }
}

let appliedId: string | null = null;
export function appliedThemeId(): string | null { return appliedId; }

/** Base theme per mode, always injected UNDER the selected theme so partial
   themes (only overriding a few tokens) still render correctly. Full themes
   simply override every token. */
export function baseThemeId(mode: 'light' | 'dark'): string {
  return mode === 'light' ? 'sakura-light' : DEFAULT_THEME_ID;
}

export function themeCssLinks(meta: ThemeMeta): string[] {
  const baseId = baseThemeId(meta.mode || 'dark');
  const links: string[] = [];
  if (meta.id !== baseId) links.push('/themes/' + encodeURIComponent(baseId) + '/theme.css');
  const base = '/themes/' + encodeURIComponent(meta.id) + '/';
  for (const f of meta.css || []) links.push(base + f);
  return links;
}

export function applyTheme(meta: ThemeMeta): void {
  if (appliedId === meta.id) return;
  appliedId = meta.id;
  document.querySelectorAll('link[data-theme-css]').forEach(e => e.remove());
  document.querySelectorAll('script[data-theme-js]').forEach(e => e.remove());
  const base = '/themes/' + encodeURIComponent(meta.id) + '/';
  for (const href of themeCssLinks(meta)) {
    const l = document.createElement('link');
    l.rel = 'stylesheet';
    l.href = href;
    l.setAttribute('data-theme-css', '');
    document.head.appendChild(l);
  }
  for (const f of meta.js || []) {
    const s = document.createElement('script');
    s.src = base + f;
    s.setAttribute('data-theme-js', '');
    document.body.appendChild(s);
  }
  // data-theme now carries the theme's mode (light/dark) — ui.css only uses it
  // for color-scheme; all color tokens come from the theme's own css.
  document.documentElement.dataset.theme = meta.mode || 'dark';
  try {
    const w = window as unknown as { SakuraAndroid?: { setBarsTheme(t: string): void } };
    if (w.SakuraAndroid) w.SakuraAndroid.setBarsTheme(meta.mode || 'dark');
  } catch (_) { /* ignore */ }
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(meta));
    localStorage.setItem(lastKey(meta.mode || 'dark'), meta.id);
  } catch (_) { /* ignore */ }
}

/* App shell: topbar + sidebar + main + bottom-nav + toast. */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Icon } from './components/Icon';
import { t } from './i18n';
import { onToast } from './toast';
import { useApp, type TabId } from './store';
import { StatsView } from './views/Stats';
import { ProvidersView } from './views/Providers';
import { ModelsView } from './views/Models';
import { StrategyView } from './views/Strategy';
import { ApiKeysView } from './views/ApiKeys';
import { SettingsView } from './views/Settings';

const TABS: { id: TabId; label: string }[] = [
  { id: 'stats', label: '统计' },
  { id: 'providers', label: '提供商' },
  { id: 'models', label: '模型' },
  { id: 'strategy', label: '策略' },
  { id: 'apikeys', label: 'API Key' },
  { id: 'settings', label: '设置' },
];

type Theme = 'dark' | 'light';

function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try { localStorage.setItem('theme', theme); } catch (_) { /* ignore */ }
  // Android shell: theme the system status/navigation bars to match.
  try {
    const w = window as unknown as { SakuraAndroid?: { setBarsTheme(t: string): void } };
    if (w.SakuraAndroid) w.SakuraAndroid.setBarsTheme(theme);
  } catch (_) { /* ignore */ }
}

function initialTheme(): Theme {
  let theme: Theme = 'dark';
  try { theme = localStorage.getItem('theme') === 'light' ? 'light' : 'dark'; } catch (_) { /* ignore */ }
  applyTheme(theme);
  return theme;
}

export function App() {
  const { state, ui, setTab, setLang } = useApp();
  const { status } = state.data;
  const { keys } = useApp().derived;
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [msg, setMsg] = useState<{ text: string; show: boolean }>({ text: '', show: false });
  const msgTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    onToast(text => {
      setMsg({ text, show: true });
      window.clearTimeout(msgTimer.current);
      msgTimer.current = window.setTimeout(() => setMsg(m => ({ ...m, show: false })), 2600);
    });
  }, []);

  const toggleTheme = useCallback(() => {
    setTheme(cur => {
      const next = cur === 'light' ? 'dark' : 'light';
      applyTheme(next);
      return next;
    });
  }, []);

  const toggleLang = useCallback(() => {
    setLang(state.ui.lang === 'zh' ? 'en' : 'zh');
  }, [setLang, state.ui.lang]);

  const ok = keys.filter(k => k.valid && !k.cool).length;
  const cool = keys.filter(k => k.cool).length;
  const off = keys.filter(k => !k.valid).length;

  return (
    <div className="app">
      {/* top bar */}
      <div className="topbar">
        <button
          className="iconbtn hamburger"
          title="收起/展开侧栏"
          aria-label="收起或展开侧栏"
          onClick={() => ui({ collapsed: !state.ui.collapsed })}
        >
          <Icon name="menu" />
        </button>
        <div className="brand">
          Sakura LLM Gateway<span className="sub">{t('控制台')}</span>
        </div>
        <div className="spacer"></div>
        <div className="run-pill">
          <span className="dot"></span>
          <span>{t('运行中')}</span>{' '}
          <code>{status ? ':' + status.ui_port : ':8001'}</code>
        </div>
        <button
          className="iconbtn"
          title="切换浅色/深色"
          aria-label="切换浅色/深色主题"
          onClick={toggleTheme}
        >
          <Icon name={theme === 'light' ? 'sun' : 'moon'} />
        </button>
        <button
          className="iconbtn lang-toggle"
          onClick={toggleLang}
          aria-label="切换语言 / switch language"
          title="切换语言 / switch language"
        >
          <Icon name="globe" />
        </button>
      </div>
      <div className="body">
        {/* vertical collapsible sidebar */}
        <aside className={'sidebar' + (state.ui.collapsed ? ' collapsed' : '')}>
          <div className="side-head"><span className="htext">{t('导航')}</span></div>
          <nav className="side-nav">
            {TABS.map(tb => (
              <button
                key={tb.id}
                className={'side-item' + (state.ui.tab === tb.id ? ' active' : '')}
                onClick={() => setTab(tb.id)}
                title={t(tb.label)}
              >
                <Icon name={tb.id} />
                <span className="side-label">{t(tb.label)}</span>
              </button>
            ))}
          </nav>
          <div className="side-foot">
            <div className="menu-card">
              <div className="mc-label">{t('Key 池')}</div>
              <div className="mc-row"><span>{t('可用')}</span><b className="mc-ok">{ok}</b></div>
              <div className="mc-row"><span>{t('冷却')}</span><b className="mc-warn">{cool}</b></div>
              <div className="mc-row"><span>{t('异常')}</span><b className="mc-err">{off}</b></div>
            </div>
            <span className="ver-chip">Sakura {status && status.version ? status.version : ''}</span>
          </div>
        </aside>
        {/* main */}
        <main className="main" tabIndex={0}>
          {status && state.ui.tab === 'stats' && <StatsView />}
          {status && state.ui.tab === 'providers' && <ProvidersView />}
          {status && state.ui.tab === 'models' && <ModelsView />}
          {status && state.ui.tab === 'strategy' && <StrategyView />}
          {status && state.ui.tab === 'apikeys' && <ApiKeysView />}
          {status && state.ui.tab === 'settings' && <SettingsView />}
        </main>
      </div>
      {/* mobile bottom tab bar (≤560px; replaces the sidebar icon-rail) */}
      <nav className="bottom-nav" aria-label="导航">
        {TABS.map(tb => (
          <button
            key={tb.id}
            className={'bn-item' + (state.ui.tab === tb.id ? ' active' : '')}
            onClick={() => setTab(tb.id)}
            aria-label={t(tb.label)}
            title={t(tb.label)}
          >
            <Icon name={tb.id} />
            <span className="bn-label">{t(tb.label)}</span>
          </button>
        ))}
      </nav>
      <div className={'msg' + (msg.show ? ' show' : '')} role="status" aria-live="polite">{msg.text}</div>
    </div>
  );
}

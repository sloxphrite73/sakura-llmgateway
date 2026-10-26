/* App shell: topbar + sidebar + main + bottom-nav + toast. */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Icon } from './components/Icon';
import { t } from './i18n';
import { onToast } from './toast';
import { lastThemeOfMode } from './theme';
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

export function App() {
  const { state, derived, ui, setTab, setLang, setTheme } = useApp();
  const { status, themes, themeId } = state.data;
  const { keys } = derived;
  const [msg, setMsg] = useState<{ text: string; show: boolean }>({ text: '', show: false });
  const msgTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    onToast(text => {
      setMsg({ text, show: true });
      window.clearTimeout(msgTimer.current);
      msgTimer.current = window.setTimeout(() => setMsg(m => ({ ...m, show: false })), 2600);
    });
  }, []);

  // Topbar sun/moon: quick-toggle between the last-used light and dark themes
  // (mode comes from each theme's manifest; defaults are the two Sakura themes).
  const curMode: 'light' | 'dark' =
    (themes.find(th => th.id === themeId) || {}).mode || 'dark';
  const toggleTheme = useCallback(() => {
    const target = lastThemeOfMode(curMode === 'light' ? 'dark' : 'light');
    if (target !== themeId) setTheme(target);
  }, [curMode, themeId, setTheme]);

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
          <Icon name={curMode === 'light' ? 'sun' : 'moon'} />
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

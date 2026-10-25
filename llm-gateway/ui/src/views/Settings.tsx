/* Settings view — globals + config import/export + Android keep-alive section.
   Port of renderSettings() / saveSettings() / importConfig() / importLegacyConfig(). */

import React, { useRef } from 'react';
import { api } from '../api';
import { t, tp } from '../i18n';
import { toast } from '../toast';
import { useApp } from '../store';

declare global {
  interface Window {
    SakuraAndroid?: {
      setBarsTheme(t: string): void;
      setLandscape(b: boolean): void;
      openBatterySettings(): void;
      openAutostartSettings(): void;
    };
  }
}

export function SettingsView() {
  const { state, refresh } = useApp();
  const s = state.data.status || ({} as Record<string, any>);
  const cooldownRef = useRef<HTMLInputElement>(null);
  const attemptsRef = useRef<HTMLInputElement>(null);
  const holdRef = useRef<HTMLInputElement>(null);
  const apiPortRef = useRef<HTMLInputElement>(null);
  const uiPortRef = useRef<HTMLInputElement>(null);
  const authRef = useRef<HTMLInputElement>(null);
  const authKeysRef = useRef<HTMLInputElement>(null);
  const importRef = useRef<HTMLInputElement>(null);
  const importLegacyRef = useRef<HTMLInputElement>(null);

  const save = async () => {
    try {
      await api('PUT', '/api/settings', {
        default_cooldown_secs: +(cooldownRef.current ? cooldownRef.current.value : 0) || 60,
        max_attempts: +(attemptsRef.current ? attemptsRef.current.value : 0) || 3,
        max_hold_secs: +(holdRef.current ? holdRef.current.value : 0),
        api_port: +(apiPortRef.current ? apiPortRef.current.value : 0) || 8000,
        ui_port: +(uiPortRef.current ? uiPortRef.current.value : 0) || 8001,
        auth: {
          enabled: !!(authRef.current && authRef.current.checked),
          keys: (authKeysRef.current ? authKeysRef.current.value : '').split(',').map(x => x.trim()).filter(Boolean),
        },
      });
      await refresh();
      toast(t('已保存'));
    } catch (e) { console.error('saveSettings', e); toast(t('失败: ') + (e as Error).message); }
  };

  const exportConfig = () => { window.open('/api/config/export', '_blank'); toast(t('已触发下载')); };

  const importConfig = async (ev: React.ChangeEvent<HTMLInputElement>) => {
    const file = ev.target.files && ev.target.files[0];
    ev.target.value = '';
    if (!file) return;
    if (!confirm(tp('导入 {n} 将整体替换当前配置（含端口与鉴权设置），确定继续？', { n: file.name }))) return;
    try {
      const content = await file.text();
      JSON.parse(content);
      await api('POST', '/api/config/import', { content });
      await refresh();
      toast(t('导入成功'));
    } catch (e) { console.error('importConfig', e); toast(t('导入失败: ') + (e as Error).message); }
  };

  const importLegacyConfig = async (ev: React.ChangeEvent<HTMLInputElement>) => {
    // Selective merge: adopt only the imported config's providers (which carry
    // keys + models). Keep the current global settings untouched.
    const file = ev.target.files && ev.target.files[0];
    ev.target.value = '';
    if (!file) return;
    if (!confirm(tp('导入旧版配置 {n}？仅采纳提供商/Key/模型/智能冷却，保留当前全局设置。', { n: file.name }))) return;
    try {
      const content = await file.text();
      JSON.parse(content);
      await api('POST', '/api/config/import-legacy', { content });
      await refresh();
      toast(t('导入成功'));
    } catch (e) { console.error('importLegacyConfig', e); toast(t('导入失败: ') + (e as Error).message); }
  };

  return (
    <>
      <h2 className="board-title">{t('设置')}</h2>
      <div className="board-note">{t('全局设置 · 端口修改后重启生效')}</div>
      <div className="section">
        <div className="section-title">{t('基本')}</div>
        <div className="card card-pad">
          <div className="form-grid">
            <label className="field"><span>{t('默认冷却时间（秒）')}</span><input ref={cooldownRef} type="number" defaultValue={s.default_cooldown_secs != null ? s.default_cooldown_secs : 60} /></label>
            <label className="field"><span>{t('每请求最大尝试次数')}</span><input ref={attemptsRef} type="number" defaultValue={s.max_attempts != null ? s.max_attempts : 3} /></label>
            <label className="field"><span>{t('全冷却最长等待（秒）')}</span><input ref={holdRef} type="number" defaultValue={s.max_hold_secs != null ? s.max_hold_secs : 120} /></label>
            <label className="field"><span>{t('API 端口')}</span><input ref={apiPortRef} type="number" defaultValue={s.api_port || 8000} /></label>
            <label className="field"><span>{t('UI 端口')}</span><input ref={uiPortRef} type="number" defaultValue={s.ui_port || 8001} /></label>
          </div>
          <div className="row" style={{ marginTop: '0.875rem' }}>
            <label className="row" style={{ gap: '0.375rem' }}>
              <input type="checkbox" ref={authRef} defaultChecked={!!(s.auth && s.auth.enabled)} />
              {' '}<span className="muted" style={{ fontSize: '0.8125rem' }}>{t('启用本地 API 鉴权')}</span>
            </label>
            <input ref={authKeysRef} placeholder={t('鉴权 Key，逗号分隔')} defaultValue={(s.auth && s.auth.keys || []).join(',')} style={{ flex: 1, minWidth: '15rem' }} />
            <button className="primary" onClick={save}>{t('保存设置')}</button>
          </div>
        </div>
      </div>
      <div className="section">
        <div className="section-title">{t('配置文件')}</div>
        <div className="card card-pad">
          <div className="row">
            <button onClick={exportConfig}>{t('导出配置')}</button>
            <button onClick={() => importRef.current && importRef.current.click()}>{t('导入配置')}</button>
            <button onClick={() => importLegacyRef.current && importLegacyRef.current.click()}>{t('导入旧版配置')}</button>
            <input type="file" ref={importRef} accept=".json,application/json" style={{ display: 'none' }} onChange={importConfig} />
            <input type="file" ref={importLegacyRef} accept=".json,application/json" style={{ display: 'none' }} onChange={importLegacyConfig} />
          </div>
          <div className="faint" style={{ fontSize: '0.75rem', marginTop: '0.625rem' }}>{t('gateway.json · 导入即时热生效')}</div>
          <div className="faint" style={{ fontSize: '0.75rem' }}>{t('旧版导入：仅采纳提供商/Key/模型/智能冷却，保留当前全局设置')}</div>
        </div>
      </div>
      {window.SakuraAndroid ? (
        <div className="section">
          <div className="section-title">{t('安卓后台保活')}</div>
          <div className="card card-pad">
            <div className="faint" style={{ fontSize: '0.8125rem' }}>{t('为防止系统（尤其小米/澎湃）清理后台：① 免除电池优化；② 允许自启动；③ 在最近任务页把本应用加锁')}</div>
            <div className="row" style={{ marginTop: '0.875rem', gap: '0.625rem', flexWrap: 'wrap' }}>
              <button onClick={() => window.SakuraAndroid!.openBatterySettings()}>{t('免除电池优化')}</button>
              <button onClick={() => window.SakuraAndroid!.openAutostartSettings()}>{t('打开自启动设置')}</button>
            </div>
            <div className="faint" style={{ fontSize: '0.75rem', marginTop: '0.625rem' }}>{t('自启动权限只能由你手动开启（系统不允许应用代开）；另外建议在最近任务页长按本应用卡片选择「锁定」。开机后网关会自动启动。')}</div>
          </div>
        </div>
      ) : null}
    </>
  );
}

/* Providers view — board + detail + add flow (choose / catalog / form).
   Port of renderProviders() / providerDetail() / renderAdd*(). */

import React, { useEffect, useRef, useState } from 'react';
import { api } from '../api';
import { Icon } from '../components/Icon';
import { t, tp } from '../i18n';
import { toast } from '../toast';
import { useApp, type AddDraft } from '../store';
import type { UiProvider } from '../types';

export function ProvidersView() {
  const { state, derived, ui, setLocalProv } = useApp();
  const { providers, keys } = derived;
  // Always render the prov-board (with the "+ 添加" head) — even when there are
  // no providers yet (fresh install / empty config).
  const sel = (state.ui.localProv && providers.find(p => p.id === state.ui.localProv))
    ? state.ui.localProv
    : (providers[0] ? providers[0].id : null);
  useEffect(() => { if (sel !== state.ui.localProv) ui({ localProv: sel }); }, [sel]);
  const p = providers.find(x => x.id === sel);
  const adding = !!state.ui.addState;

  return (
    <>
      <h2 className="board-title prov-board-title">{t('提供商')}</h2>
      <div className={'prov-board' + (providers.length ? '' : ' empty') + (adding ? ' adding' : '')}>
        <div className="prov-list-col">
          <div className="pl-head">
            <span className="pl-head-label">{t('提供商')}</span>
            <span className="add" onClick={() => ui({ addState: 'choose', addDraft: null })}>{t('+ 添加')}</span>
          </div>
          {providers.length ? providers.map(pr => {
            const kc = keys.filter(k => k.prov === pr.id).length;
            return (
              <div
                key={pr.id}
                className={'pl-item' + (sel === pr.id ? ' active' : '')}
                onClick={() => setLocalProv(pr.id)}
              >
                <span className="pdot" style={{ background: pr.dot }}></span>
                <span className="pl-name">{pr.name}</span>
                <span className="pl-badge">{kc}</span>
              </div>
            );
          }) : <div className="faint pl-empty">{t('暂无提供商，点击「+ 添加」创建')}</div>}
        </div>
        <div className="prov-detail-col">
          {adding ? <AddView /> : (p ? (
            <>
              <h2 className="board-title">{p.name}</h2>
              <div className="board-note">{t('基本信息 + 限额（已归并到提供商下）')}</div>
              <ProviderDetail key={p.id} p={p} />
            </>
          ) : (
            <>
              <h2 className="board-title">{t('提供商')}</h2>
              <div className="board-note">{t('暂无提供商，点击「+ 添加」创建')}</div>
            </>
          ))}
        </div>
      </div>
    </>
  );
}

function ProviderDetail({ p }: { p: UiProvider }) {
  const { derived, refresh } = useApp();
  const { models, keys } = derived;
  const kc = keys.filter(k => k.prov === p.id).length;
  const mc = models.filter(mo => mo.prov === p.id).length;
  const dm = p.detectedMetrics || { rpm: 0, tpm: 0 };
  const liveR = dm.rpm, liveT = dm.tpm;
  const [autoR, setAutoR] = useState(p.rpmLimit == null);
  const [autoT, setAutoT] = useState(p.tpmLimit == null);
  const nameRef = useRef<HTMLInputElement>(null);
  const urlRef = useRef<HTMLInputElement>(null);
  const protoRef = useRef<HTMLSelectElement>(null);
  const rpmRef = useRef<HTMLInputElement>(null);
  const tpmRef = useRef<HTMLInputElement>(null);

  const save = async () => {
    try {
      await api('PUT', '/api/providers/' + p.id, {
        name: nameRef.current ? nameRef.current.value : p.name,
        base_url: urlRef.current ? urlRef.current.value : p.base,
        protocol: protoRef.current ? protoRef.current.value : p.proto,
      });
      await api('PUT', '/api/providers/' + p.id + '/rate-limits', {
        rpm_limit: autoR ? null : Number(rpmRef.current ? rpmRef.current.value : 0),
        tpm_limit: autoT ? null : Number(tpmRef.current ? tpmRef.current.value : 0),
      });
      await refresh();
      toast(t('已保存'));
    } catch (e) {
      console.error('saveProvider', e);
      toast(t('失败: ') + (e as Error).message);
    }
  };

  const del = async () => {
    if (!confirm(t('删除该提供商及其所有 Key？'))) return;
    try {
      await api('DELETE', '/api/providers/' + p.id);
      await refresh();
      toast(t('已删除'));
    } catch (e) {
      console.error('delProvider', e);
      toast(t('失败: ') + (e as Error).message);
    }
  };

  return (
    <div className="pd-card">
      <div className="section">
        <div className="section-title">{t('基本信息')}</div>
        <div className="card card-pad">
          <div className="pd-field">
            <span className="lbl">{t('名称')}</span>
            <input className="val" ref={nameRef} aria-label={t('名称')} defaultValue={p.name} />
          </div>
          <div className="pd-field">
            <span className="lbl">{t('Base URL')}</span>
            <input className="val mono" ref={urlRef} aria-label={t('Base URL')} defaultValue={p.base} />
          </div>
          <div className="pd-field">
            <span className="lbl">{t('协议')}</span>
            <select className="val" ref={protoRef} aria-label={t('协议')} defaultValue={p.proto}>
              <option value="openai">openai</option>
              <option value="anthropic">anthropic</option>
            </select>
          </div>
          <div className="faint" style={{ fontSize: '0.75rem', marginTop: '0.25rem' }}>
            {tp('托管模型 {mc} 个 · Key {kc} 把（Key 在 API Key 板统一管理）', { mc, kc })}
          </div>
        </div>
      </div>
      <div className="section">
        <div className="section-title">{t('配额与速率')} <span className="note">{t('RPM/TPM 可手填或采用自动检测')}</span></div>
        <div className="card card-pad">
          <div className="pd-field pd-row">
            <span className="lbl">{t('RPM')}</span>
            <input
              className={'val' + (autoR ? ' locked' : '')}
              ref={rpmRef}
              aria-label={t('RPM')}
              type="number" min={0}
              key={'rpm' + (autoR ? '-a' : '-m')}
              defaultValue={autoR ? liveR : (p.rpmLimit || 0)}
              readOnly={autoR}
            />
            <label className="toggle">
              <input
                type="checkbox"
                checked={autoR}
                onChange={e => setAutoR(e.target.checked)}
              />
              <span className="track"></span><span className="slabel">{t('采用自动检测结果')}</span>
            </label>
          </div>
          <div className="pd-field pd-row">
            <span className="lbl">{t('TPM')}</span>
            <input
              className={'val' + (autoT ? ' locked' : '')}
              ref={tpmRef}
              aria-label={t('TPM')}
              type="number" min={0}
              key={'tpm' + (autoT ? '-a' : '-m')}
              defaultValue={autoT ? liveT : (p.tpmLimit || 0)}
              readOnly={autoT}
            />
            <label className="toggle">
              <input
                type="checkbox"
                checked={autoT}
                onChange={e => setAutoT(e.target.checked)}
              />
              <span className="track"></span><span className="slabel">{t('采用自动检测结果')}</span>
            </label>
          </div>
          <div className="pd-field pd-row">
            <span className="lbl">{t('配额重置时间')}</span>
            <input className="val" aria-label={t('配额重置时间')} defaultValue={p.resetAt || ''} />
          </div>
          <div className="pd-field pd-row">
            <span className="lbl">{t('重置配额')}</span>
            <input className="val" aria-label={t('重置配额（token）')} type="number" defaultValue={p.resetQuota || 0} />
          </div>
        </div>
      </div>
      <div className="row" style={{ marginTop: '1.5rem', gap: '0.625rem' }}>
        <button className="primary" onClick={save}>{t('保存')}</button>
        <button
          className="small"
          style={{ borderColor: 'var(--color-feedback-error-text)', color: 'var(--color-feedback-error-text)' }}
          onClick={del}
        >{t('删除提供商')}</button>
      </div>
    </div>
  );
}

/* ---------- add provider (catalog pick | custom) ---------- */

function AddView() {
  const { state } = useApp();
  if (state.ui.addState === 'choose') return <AddChoose />;
  if (state.ui.addState === 'catalog') return <AddCatalog />;
  if (state.ui.addState === 'form') return <AddForm />;
  return null;
}

function AddChoose() {
  const { ui } = useApp();
  return (
    <>
      <div className="edit-head">
        <button className="ghost" onClick={() => ui({ addState: null })}>{t('← 返回')}</button>
      </div>
      <h2 className="board-title">{t('添加提供商')}</h2>
      <div className="board-note">{t('从免费目录选一家，或自定义填入')}</div>
      <div className="add-choose">
        <button
          className="add-pick"
          aria-label={t('自定义提供商')}
          onClick={() => ui({
            addDraft: { name: '', base_url: '', protocol: 'openai', models: [], key: '', source: 'custom' },
            addState: 'form',
          })}
        >
          <span className="ap-icon"><Icon name="addCustom" /></span>
          <span className="ap-text">
            <span className="ap-title">{t('自定义提供商')}</span>
            <span className="ap-desc">{t('手动填写 base_url、协议与模型')}</span>
          </span>
          <span className="ap-arrow">→</span>
        </button>
        <button
          className="add-pick"
          aria-label={t('从免费目录选择')}
          onClick={() => ui({ addState: 'catalog' })}
        >
          <span className="ap-icon"><Icon name="addCatalog" /></span>
          <span className="ap-text">
            <span className="ap-title">{t('已有的提供商')}</span>
            <span className="ap-desc">{t('从免费目录选一家，预填 base_url / 协议 / 模型')}</span>
          </span>
          <span className="ap-arrow">→</span>
        </button>
      </div>
    </>
  );
}

function AddCatalog() {
  const { state, ui } = useApp();
  const catalog = state.data.catalog;
  const lang = state.ui.lang;
  return (
    <>
      <div className="edit-head">
        <button className="ghost" onClick={() => ui({ addState: 'choose' })}>{t('← 返回')}</button>
      </div>
      <h2 className="board-title">{t('已有的提供商')}</h2>
      <div className="board-note">{t('免费目录 · 点一张预填配置')}</div>
      <div className="mc-grid">
        {catalog.map((c, i) => (
          <div
            className="mc"
            key={i}
            onClick={() => ui({
              addDraft: {
                name: c.name, base_url: c.base_url, protocol: c.protocol,
                models: (c.free_models || []).slice(), key: '', source: 'catalog',
              },
              addState: 'form',
            })}
          >
            <div className="mc-top"><div>
              <div className="mc-name">{c.name}</div>
              <div className="mc-prov">{c.base_url}</div>
            </div></div>
            <div className="mc-meta">
              <span>{t('模型')} <b>{(c.free_models || []).length}</b></span>
              <span>{t('协议')} <b>{c.protocol}</b></span>
            </div>
            <div className="mc-prov">
              {lang === 'en' ? (c.notes_en || c.notes_zh || c.note || '') : (c.notes_zh || c.notes_en || c.note || '')}
            </div>
            <span className="mc-arrow">→</span>
          </div>
        ))}
        {catalog.length === 0 && <div className="empty">{t('暂无数据')}</div>}
      </div>
    </>
  );
}

function AddForm() {
  const { state, ui, refresh } = useApp();
  const d = state.ui.addDraft as AddDraft;
  const isCatalog = d.source === 'catalog';
  const valid = !!(d.name.trim() && d.base_url.trim() && d.key.trim());
  const modelInputRef = useRef<HTMLInputElement>(null);

  const patch = (p: Partial<AddDraft>) => ui({ addDraft: { ...d, ...p } });

  const addModel = () => {
    const inp = modelInputRef.current;
    if (!inp) return;
    const v = inp.value.trim();
    if (v && !d.models.includes(v)) patch({ models: [...d.models, v] });
    inp.value = '';
  };

  const commit = async () => {
    if (!valid) return;
    try {
      const cfg = await api<{ providers?: { id: string; name: string }[] }>('POST', '/api/providers', {
        name: d.name.trim(),
        base_url: d.base_url.trim().replace(/\/+$/, ''),
        protocol: d.protocol,
      });
      const prov = (cfg.providers || []).find(x => x.name === d.name.trim());
      if (!prov) throw new Error('provider create failed');
      const pid = prov.id;
      await api('POST', '/api/providers/' + pid + '/keys', { key: d.key.trim(), display_name: '' });
      for (const mId of d.models) {
        try { await api('POST', '/api/providers/' + pid + '/models', { id: mId }); }
        catch (e) { /* ignore duplicate-model conflicts */ }
      }
      ui({ addState: null, addDraft: null, localProv: pid });
      await refresh();
      toast(t('添加成功'));
    } catch (e) {
      console.error('commitAdd', e);
      toast(t('添加失败: ') + (e as Error).message);
    }
  };

  const back = () => {
    if (isCatalog) ui({ addState: 'catalog' });
    else ui({ addState: null });
  };

  const paste = async () => {
    try {
      const txt = await navigator.clipboard.readText();
      patch({ key: txt });
    } catch (e) { /* ignore */ }
  };

  return (
    <>
      <div className="edit-head">
        <button className="ghost" onClick={back}>{t('← 返回')}</button>
      </div>
      <h2 className="board-title">{isCatalog ? t('添加：') + d.name : t('自定义提供商')}</h2>
      <div className="board-note">{isCatalog ? t('已从目录预填，可改 · 粘 Key 后添加') : t('手动填写 · 粘 Key 后添加')}</div>
      <div className="card card-pad">
        <div className="pd-field">
          <span className="lbl">{t('名称')}</span>
          <input className="val" aria-label={t('名称')} value={d.name} onChange={e => patch({ name: e.target.value })} />
        </div>
        <div className="pd-field">
          <span className="lbl">{t('Base URL')}</span>
          <input className="val mono" aria-label={t('Base URL')} value={d.base_url} onChange={e => patch({ base_url: e.target.value })} />
        </div>
        <div className="pd-field">
          <span className="lbl">{t('协议')}</span>
          <select className="val" aria-label={t('协议')} value={d.protocol} onChange={e => patch({ protocol: e.target.value })}>
            <option value="openai">openai</option>
            <option value="anthropic">anthropic</option>
          </select>
        </div>
        <div className="pd-field">
          <span className="lbl">{t('托管模型')}</span>
          <div className="alias-list">
            {d.models.map((m, i) => (
              <span className="keytag" key={m}>
                {m}{' '}
                <a
                  href="#"
                  aria-label={t('删除模型')}
                  className="alias-x"
                  style={{ color: 'var(--color-feedback-error-text)', textDecoration: 'none' }}
                  onClick={e => { e.preventDefault(); patch({ models: d.models.filter((_, j) => j !== i) }); }}
                ><Icon name="x" /></a>
              </span>
            ))}
            <input
              ref={modelInputRef}
              aria-label={t('添加模型')}
              placeholder={t('添加模型 id')}
              style={{ maxWidth: '11rem' }}
              onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); addModel(); } }}
            />
            <button className="small" onClick={addModel}>{t('添加')}</button>
          </div>
        </div>
        <div className="pd-field">
          <span className="lbl">{t('API Key')}</span>
          <span className="ak-input-wrap">
            <input
              className="ak-key-input"
              aria-label={t('API Key')}
              placeholder={t('粘贴 API Key')}
              value={d.key}
              onChange={e => patch({ key: e.target.value })}
            />
            <button className="ak-paste" type="button" aria-label={t('从剪贴板粘贴')} title={t('从剪贴板粘贴')} onClick={paste}>
              <Icon name="clip" />
            </button>
          </span>
        </div>
      </div>
      <div className="row" style={{ marginTop: '1.5rem', gap: '0.625rem' }}>
        <button className="primary" onClick={commit} disabled={!valid}>{t('添加提供商')}</button>
        <button className="small" onClick={back}>{t('取消')}</button>
      </div>
    </>
  );
}

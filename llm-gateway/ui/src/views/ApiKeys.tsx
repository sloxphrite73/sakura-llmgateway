/* API Keys view — global key table + add card + recycle + edit modal.
   Port of renderApiKeys() / ak* actions / renderAkSeedCard().
   Cooldown countdown derives from data.now (1s tick in the store); when a
   cooldown hits 0 the store's tick triggers one debounced refreshAkStatus. */

import React, { useRef, useState } from 'react';
import { api, copyText } from '../api';
import { Icon } from '../components/Icon';
import { t, tp } from '../i18n';
import { toast } from '../toast';
import { useApp } from '../store';
import type { UiKey } from '../types';

function keyStatus(k: UiKey, cd: number) {
  if (!k.valid) return <span className="st off">{t('异常')}</span>;
  if (k.cool && cd > 0) return <span className="st cool">{tp('冷却 · 剩 {cd}s', { cd })}</span>;
  if (k.cool) return <span className="st cool">{tp('冷却 · 剩 {cd}s', { cd: 0 })}</span>;
  return <span className="st ok">{t('可用')}</span>;
}

export function ApiKeysView() {
  const { state, derived, ui, refresh, pname } = useApp();
  const { keys, providers } = derived;
  const now = state.data.now;
  const provSelRef = useRef<HTMLSelectElement>(null);
  const keyInputRef = useRef<HTMLTextAreaElement>(null);

  const chips: [string, string][] = [['all', t('全部')], ['ok', t('可用')], ['cool', t('冷却中')], ['off', t('异常')]];
  const f = state.ui.keyFilter;
  const list = keys.filter(k => f === 'all' || (f === 'ok' && k.valid && !k.cool) || (f === 'cool' && k.cool) || (f === 'off' && !k.valid));
  const invCount = keys.filter(k => !k.valid).length;

  const liveCd = (k: UiKey) => k.cool ? Math.max(0, Math.ceil((k.untilMs - now) / 1000)) : 0;

  const akPaste = async () => {
    try {
      const txt = await navigator.clipboard.readText();
      if (keyInputRef.current) keyInputRef.current.value = txt;
    } catch (e) { /* ignore */ }
  };

  const akRecycle = async () => {
    const inv = keys.filter(k => !k.valid);
    if (!inv.length) return;
    for (const k of inv) {
      try { await api('DELETE', '/api/providers/' + k.prov + '/keys/' + encodeURIComponent(k.kid)); }
      catch (e) { console.error('akRecycle', e); }
    }
    ui({ recycled: (state.ui.recycled || 0) + inv.length });
    await refresh();
    toast(t('已删除'));
  };

  const akAdd = async () => {
    const sel = provSelRef.current, inp = keyInputRef.current;
    if (!sel || !inp) return;
    const prov = sel.value, raw = inp.value.trim();
    if (!raw) return;
    // Always split by newlines/commas — single key or batch, same path.
    // 每行可带显示名：最后一个空格前的是名字，之后的是 Key（Key 本身不含空格）。
    const toAdd = raw.split(/[\n,]+/).map(s => s.trim()).filter(Boolean).map(line => {
      const i = line.lastIndexOf(' ');
      return i > 0
        ? { key: line.slice(i + 1).trim(), display_name: line.slice(0, i).trim() }
        : { key: line, display_name: '' };
    });
    let failed = 0;
    for (const k of toAdd) {
      try { await api('POST', '/api/providers/' + prov + '/keys', k); }
      catch (e) { failed++; console.error('akAdd', e); }
    }
    inp.value = '';
    await refresh();
    if (failed) toast(t('失败: ') + failed);
    else if (toAdd.length > 1) toast(tp('已添加 {n} 把', { n: toAdd.length - failed }));
  };

  return (
    <div className="ak-layout">
      <h2 className="board-title">{t('API Key')}</h2>
      <div className="board-note">
        {t('全局状态总览 · 跨所有提供商一张表')}
        {state.ui.recycled ? ` · ${tp('已回收 {n} 把', { n: state.ui.recycled })}` : ''}
      </div>
      {state.ui.akEdit ? <AkEditModal kid={state.ui.akEdit} /> : null}
      <div className="card card-pad" style={{ marginBottom: '1.25rem', flex: 'none' }}>
        <div className="ak-add">
          <select ref={provSelRef} aria-label={t('提供商')}>
            {providers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
          <span className="ak-input-wrap">
            <textarea
              ref={keyInputRef}
              className="ak-key-input"
              aria-label={t('API Key')}
              placeholder={t('粘贴 API Key，多个每行一个；可在 Key 前加名字，空格分隔')}
              rows={2}
            ></textarea>
            <button className="ak-paste" type="button" aria-label={t('从剪贴板粘贴')} title={t('从剪贴板粘贴')} onClick={akPaste}>
              <Icon name="clip" />
            </button>
          </span>
          <button className="primary small" type="button" onClick={akAdd}>{t('添加')}</button>
        </div>
        <div className="ak-recycle" style={{ marginTop: '0.875rem' }}>
          <button className="small" type="button" onClick={akRecycle} disabled={!invCount}>{t('移动所有无效Key至回收站')}</button>
        </div>
      </div>
      <div className="chips" style={{ marginBottom: '0.875rem', flex: 'none' }}>
        {chips.map(c => (
          <span key={c[0]} className={'chip' + (f === c[0] ? ' active' : '')} onClick={() => ui({ keyFilter: c[0] })}>{c[1]}</span>
        ))}
      </div>
      <div className="card ak-table-card"><div className="tbl-scroll"><table className="cards">
        <tbody>
          <tr>
            <th>{t('名字')}</th><th>{t('Key')}</th><th>{t('提供商')}</th><th>{t('智能冷却时长')}</th><th>{t('状态')}</th>
            <th>RPM</th><th>TPM</th><th>{t('利用率')}</th><th>{t('avg_TFTT')}</th>
            <th>{t('token 余额')}</th><th>{t('账单')}</th><th>{t('请求数')}</th><th>{t('最近错误')}</th><th>{t('操作')}</th>
          </tr>
          {list.map(k => <KeyRow key={k.kid} k={k} cd={liveCd(k)} pname={pname} />)}
          {list.length === 0 && <tr><td colSpan={14} className="faint">{t('无')}</td></tr>}
        </tbody>
      </table></div></div>
    </div>
  );
}

function KeyRow({ k, cd, pname }: { k: UiKey; cd: number; pname: (id: string) => string }) {
  const { refresh, findRealKey, ui } = useApp();
  // shown: real key text while revealed; survives re-renders (row keyed by kid).
  const [shown, setShown] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const akShow = async () => {
    if (shown != null) { setShown(null); return; }
    try {
      const real = await findRealKey(k.kid);
      if (!real) { toast(t('未找到该 Key')); return; }
      setShown(real);
    } catch (e) { console.error('akShow', e); toast(t('失败: ') + (e as Error).message); }
  };
  const akCopy = async () => {
    try {
      const real = await findRealKey(k.kid);
      if (!real) { toast(t('未找到该 Key')); return; }
      if (navigator.clipboard) navigator.clipboard.writeText(real).catch(() => {});
      else copyText(real);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch (e) { console.error('akCopy', e); toast(t('失败: ') + (e as Error).message); }
  };
  const akDel = async () => {
    if (!confirm(t('删除该 Key？'))) return;
    try { await api('DELETE', '/api/providers/' + k.prov + '/keys/' + encodeURIComponent(k.kid)); await refresh(); toast(t('已删除')); }
    catch (e) { console.error('akDel', e); toast(t('失败: ') + (e as Error).message); }
  };

  const dn = k.dname || k.masked || '••••';
  return (
    <tr className={!k.valid ? 'ak-off' : (k.cool && cd > 0 ? 'ak-cool' : '')}>
      <td data-label={t('名字')}><span className="ak-dname" title={dn}>{dn}</span></td>
      <td data-label={t('Key')}><span className="keytag">{shown != null ? shown : (k.masked || '••••')}</span></td>
      <td data-label={t('提供商')}>{pname(k.prov)}</td>
      <td data-label={t('智能冷却时长')} className="num">{k.cdDur}s</td>
      <td data-label={t('状态')}>{keyStatus(k, cd)}</td>
      <td data-label="RPM" className="num">{k.rpm || 0}</td>
      <td data-label="TPM" className="num">{k.tpm || 0}</td>
      <td
        data-label={t('利用率')}
        className="num"
        title={(k.capRpm || k.capTpm) ? t('实测容量') + `: RPM ${k.capRpm || '-'} / TPM ${k.capTpm || '-'}` : ''}
      >{k.util != null ? Math.round(k.util * 100) + '%' : '-'}</td>
      <td data-label={t('avg_TFTT')} className="num">{k.tftt ? k.tftt + 'ms' : '-'}</td>
      <td data-label={t('token 余额')} className="num">{k.tb === Infinity ? '—' : k.tb.toLocaleString()}</td>
      <td data-label={t('账单')} className="num">{k.bb === Infinity ? '—' : '¥' + k.bb}</td>
      <td data-label={t('请求数')} className="num">{k.req || 0}</td>
      <td data-label={t('最近错误')} className="faint"><span className="ak-err" title={k.lastErr || '-'}>{k.lastErr || '-'}</span></td>
      <td data-label={t('操作')} className="ak-actions">
        <button className="small" type="button" onClick={akShow}>{shown != null ? t('隐藏') : t('显示')}</button>
        <button className="small" type="button" onClick={akCopy}>{copied ? t('已复制') : t('复制')}</button>
        <button className="small" type="button" onClick={() => openEdit(k, ui, findRealKey)}>{t('编辑')}</button>
        <button
          className="small"
          type="button"
          onClick={akDel}
          style={{ borderColor: 'var(--color-feedback-error-text)', color: 'var(--color-feedback-error-text)' }}
        >{t('删除')}</button>
      </td>
    </tr>
  );
}

async function openEdit(
  k: UiKey,
  ui: (p: { akEdit: string | null; akEditKey: string | null }) => void,
  findRealKey: (id: string) => Promise<string | null>,
) {
  ui({ akEdit: k.kid, akEditKey: '' });
  // 打开后异步取回 Key 本体填入（导出接口拿全量，幂等只读）
  try {
    const real = await findRealKey(k.kid);
    if (real) ui({ akEdit: k.kid, akEditKey: real });
  } catch (_) { /* ignore */ }
}

/* ---------- Key 编辑弹窗：显示名 + Key 本体（一行一个元素，标签在输入框上方） ---------- */
function AkEditModal({ kid }: { kid: string }) {
  const { state, derived, ui, refresh } = useApp();
  const k = derived.keys.find(x => x.kid === kid);
  if (!k) return null;

  const close = () => ui({ akEdit: null, akEditKey: null });
  const save = async () => {
    const body = {
      display_name: (document.getElementById('ak-dname') as HTMLInputElement | null)?.value.trim() ?? '',
      key: state.ui.akEditKey ? state.ui.akEditKey.trim() : '',
    };
    try {
      await api('PUT', '/api/providers/' + k.prov + '/keys/' + encodeURIComponent(kid), body);
      ui({ akEdit: null, akEditKey: null });
      await refresh();
      toast(t('已保存'));
    } catch (e) { console.error('saveAkSeed', e); toast(t('失败: ') + (e as Error).message); }
  };

  return (
    <div className="ak-modal-backdrop" onClick={e => { if (e.target === e.currentTarget) close(); }}>
      <div className="card card-pad ak-modal">
        <div className="edit-card">
          <div className="edit-name grp-sans">{t('编辑Key信息')}</div>
          <label className="field">
            <span>{t('名字')}</span>
            <input className="val" id="ak-dname" type="text" aria-label={t('名字')} placeholder={t('留空 = 显示掩码 Key')} defaultValue={k.dname || ''} />
          </label>
          <label className="field">
            <span>Key</span>
            <input
              className="val mono"
              id="ak-key-edit"
              type="text"
              aria-label="Key"
              value={state.ui.akEditKey || ''}
              onChange={e => ui({ akEditKey: e.target.value })}
            />
          </label>
          <div className="ak-actions" style={{ marginTop: '1.5rem' }}>
            <button className="primary small" type="button" onClick={save}>{t('保存')}</button>
            <button className="small" type="button" onClick={close}>{t('取消')}</button>
          </div>
        </div>
      </div>
    </div>
  );
}

/* Models view — info list / edit / model-group editor.
   Port of renderModels() / renderModelList() / renderModelEdit() /
   renderModelGroups() / renderGroupEditor(). */

import React, { useRef, useState } from 'react';
import { api } from '../api';
import { Icon } from '../components/Icon';
import { t, tp } from '../i18n';
import { toast } from '../toast';
import { useApp, type GroupEdit } from '../store';
import type { UiModel } from '../types';

export function ModelsView() {
  const { state, setModelTab } = useApp();
  return (
    <>
      <h2 className="board-title">{t('模型')}</h2>
      <div className="board-note">{t('模型信息：所有模型条目大列表，点一条进编辑页 · 模型组：跨提供商同模型编组')}</div>
      <div className="subtabs">
        <button className={state.ui.modelTab === 'info' ? 'active' : ''} onClick={() => setModelTab('info')}>{t('模型信息')}</button>
        <button className={state.ui.modelTab === 'groups' ? 'active' : ''} onClick={() => setModelTab('groups')}>{t('模型组管理')}</button>
      </div>
      {/* #model-pane wrapper is load-bearing: .mf-row's own margin shorthand
          overrides `.main > *` centering, so list/grid must live one level down
          (same DOM shape as the original renderModels → renderModelList). */}
      <div id="model-pane">
        {state.ui.editingModel != null
          ? <ModelEdit />
          : state.ui.modelTab === 'info' ? <ModelList /> : <ModelGroups />}
      </div>
    </>
  );
}

async function toggleModelOn(mo: UiModel, refresh: () => Promise<void>) {
  try { await api('PUT', '/api/providers/' + mo.prov + '/models/' + encodeURIComponent(mo.id)); await refresh(); }
  catch (e) { console.error('toggleModelOn', e); toast(t('失败: ') + (e as Error).message); }
}
async function toggleModelImg(mo: UiModel, refresh: () => Promise<void>) {
  try { await api('PUT', '/api/providers/' + mo.prov + '/models/' + encodeURIComponent(mo.id) + '/supports-images', { supports_images: !mo.img }); await refresh(); }
  catch (e) { console.error('toggleModelImg', e); toast(t('失败: ') + (e as Error).message); }
}

function ModelList() {
  const { state, derived, ui, refresh } = useApp();
  const { models, providers } = derived;
  const list = state.ui.modelFilter === 'all' ? models : models.filter(mo => mo.prov === state.ui.modelFilter);
  const fchips: [string, string][] = [['all', t('全部')], ...providers.map(p => [p.id, p.name.split(' ')[0]] as [string, string])];
  const [fetching, setFetching] = useState(false);
  const provSelRef = useRef<HTMLSelectElement>(null);

  const fetchUpstreamModels = async () => {
    const pid = provSelRef.current ? provSelRef.current.value : '';
    if (!pid) { toast(t('请先选择提供商')); return; }
    setFetching(true);
    let modelsUp: string[];
    try {
      const r = await api<{ models?: string[] }>('GET', '/api/providers/' + pid + '/upstream-models');
      modelsUp = r.models || [];
    } catch (e) {
      setFetching(false);
      toast(t('拉取失败: ') + (e as Error).message);
      return;
    }
    setFetching(false);
    if (!modelsUp.length) { toast(t('上游未返回任何模型')); return; }
    let n = 0;
    try {
      const r = await api<{ imported?: number }>('POST', '/api/providers/' + pid + '/models/import', { models: modelsUp });
      n = r.imported || 0;
    } catch (e) { toast(t('失败: ') + (e as Error).message); return; }
    await refresh();
    toast(tp('新添加 {n} 个模型', { n }));
  };

  return (
    <>
      <div className="chips" style={{ marginBottom: '1rem' }}>
        {fchips.map(fc => (
          <span
            key={fc[0]}
            className={'chip' + (state.ui.modelFilter === fc[0] ? ' active' : '')}
            onClick={() => ui({ modelFilter: fc[0] })}
          >{fc[1]}</span>
        ))}
      </div>
      <div className="mf-row">
        <div className="mf-left">
          <span className="faint" style={{ fontSize: '0.75rem' }}>{t('提供商')}</span>
          <select
            ref={provSelRef}
            aria-label={t('提供商')}
            value={state.ui.modelFilter === 'all' ? (providers[0] ? providers[0].id : '') : state.ui.modelFilter}
            onChange={e => ui({ modelFilter: e.target.value })}
          >
            {providers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
          </select>
        </div>
        <button onClick={fetchUpstreamModels} disabled={fetching}>{fetching ? t('拉取中…') : t('从上游获取模型')}</button>
      </div>
      <div className="mc-grid">
        {list.map(mo => {
          const idx = models.indexOf(mo);
          return (
            <div key={mo.prov + ':' + mo.id} className={'mc' + (mo.on ? '' : ' disabled')} onClick={() => ui({ editingModel: idx })}>
              <div className="mc-top">
                <div>
                  <div className="mc-name">{mo.id}</div>
                  <div className="mc-prov">
                    {derived.providers.find(p => p.id === mo.prov)?.name || mo.prov}
                    {mo.alias.length ? ` · ↔ ${mo.alias.join(', ')}` : ''}
                  </div>
                </div>
                <span
                  className={'mc-sw' + (mo.on ? ' on' : '')}
                  onClick={e => { e.stopPropagation(); toggleModelOn(mo, refresh); }}
                  title={t('启用/禁用')}
                >
                  <span className="track"></span><span className="slabel">{mo.on ? t('启用') : t('禁用')}</span>
                </span>
              </div>
              <div className="mc-meta">
                <span>{t('上下文')} <b>{(mo.ctx / 1000).toFixed(0)}k</b></span>
                <span>{t('输入')} <b>¥{mo.pIn}</b></span>
                <span>{t('输出')} <b>¥{mo.pOut}</b></span>
              </div>
              <div className="mc-hover">
                <span>avgTPS <b>{mo.tps ? mo.tps.toFixed(1) : '-'}</b></span>
                <span>avgTFTT <b>{mo.tftt ? mo.tftt + 'ms' : '-'}</b></span>
              </div>
              <span className="mc-arrow">→</span>
            </div>
          );
        })}
        {list.length === 0 && <div className="empty">{t('无')}</div>}
      </div>
    </>
  );
}

function ModelEdit() {
  const { state, derived, ui, refresh } = useApp();
  const { models, keys } = derived;
  const idx = state.ui.editingModel;
  const mo = idx != null ? models[idx] : undefined;
  const ctxRef = useRef<HTMLInputElement>(null);
  const priceRef = useRef<HTMLInputElement>(null);
  const poutRef = useRef<HTMLInputElement>(null);
  const aliasRef = useRef<HTMLInputElement>(null);
  if (!mo) return null;
  const i = idx as number;

  const addAlias = async () => {
    const inp = aliasRef.current;
    if (!inp) return;
    const a = inp.value.trim();
    if (!a) return;
    try {
      await api('POST', '/api/providers/' + mo.prov + '/aliases', { alias: a, model: mo.id });
      inp.value = '';
      await refresh();
    } catch (e) { console.error('addModelAlias', e); toast(t('失败: ') + (e as Error).message); }
  };
  const delAlias = async (alias: string) => {
    try { await api('DELETE', '/api/providers/' + mo.prov + '/aliases/' + encodeURIComponent(alias)); await refresh(); }
    catch (e) { console.error('delModelAlias', e); toast(t('失败: ') + (e as Error).message); }
  };
  const save = async () => {
    const v = priceRef.current ? parseFloat(priceRef.current.value) : NaN;
    const price = isNaN(v) ? 0 : v;
    const ov = poutRef.current ? parseFloat(poutRef.current.value) : NaN;
    const outPrice = isNaN(ov) ? 0 : ov;
    const kv = ctxRef.current ? parseFloat(ctxRef.current.value) : NaN;
    // UI holds K units → ×1000 to tokens; 0/empty = clear (None).
    const ctxTokens = (!isNaN(kv) && kv > 0) ? Math.round(kv * 1000) : null;
    // build the full price tables (existing entries + this model) and PUT both
    const p = ((state.data.status && state.data.status.providers) || []).find(x => x.id === mo.prov);
    const table = Object.assign({}, (p && p.price_table) || {});
    table[mo.id] = price;
    const outTable = Object.assign({}, (p && p.output_price_table) || {});
    outTable[mo.id] = outPrice;
    try {
      await api('PUT', '/api/providers/' + mo.prov + '/price-table', { table, output_table: outTable });
      await api('PUT', '/api/providers/' + mo.prov + '/models/' + encodeURIComponent(mo.id) + '/context-length', { context_length: ctxTokens });
      await refresh();
      toast(t('已保存'));
    } catch (e) { console.error('saveModelEdit', e); toast(t('失败: ') + (e as Error).message); }
  };

  return (
    <>
      <div className="edit-head">
        <button className="ghost" onClick={() => ui({ editingModel: null })}>{t('← 返回')}</button>
      </div>
      <div className="edit-card">
        <div className="edit-name">{mo.id}</div>
        <div className="edit-sub">
          {derived.providers.find(p => p.id === mo.prov)?.name || mo.prov} · {t('上游')} {mo.up}
          {mo.alias.length ? ` · ↔ ${mo.alias.join(', ')}` : ''}
        </div>
        <div className="card card-pad">
          <div className="erow"><span className="elbl">{t('模型 ID')}</span><input className="eval" aria-label={t('模型 ID')} defaultValue={mo.id} readOnly title={t('无后端')} /></div>
          <div className="erow"><span className="elbl">{t('上游模型名')}</span><input className="eval" aria-label={t('上游模型名')} defaultValue={mo.up} readOnly title={t('无后端')} /></div>
          <div className="erow">
            <span className="elbl">{t('上下文长度')}</span>
            <input className="eval" ref={ctxRef} aria-label={t('上下文长度')} type="number" min={0} step={1} defaultValue={mo.ctx ? Math.round(mo.ctx / 1000) : 0} />
            <span className="eunit">{t('K')}</span>
          </div>
          <div className="erow">
            <span className="elbl">{t('输入价格')}</span>
            <input className="eval" ref={priceRef} aria-label={t('输入价格')} type="number" step={0.01} defaultValue={mo.pIn} />
            <span className="eunit">{t('¥/1M tok')}</span>
          </div>
          <div className="erow">
            <span className="elbl">{t('输出价格')}</span>
            <input className="eval" ref={poutRef} aria-label={t('输出价格')} type="number" step={0.01} defaultValue={mo.pOut} />
            <span className="eunit">{t('¥/1M tok')}</span>
          </div>
          <div className="erow">
            <span className="elbl">{t('启用')}</span>
            <span className={'mc-sw' + (mo.on ? ' on' : '')} onClick={() => toggleModelOn(mo, refresh)}>
              <span className="track"></span><span className="slabel">{mo.on ? t('是') : t('否')}</span>
            </span>
          </div>
          <div className="erow">
            <span className="elbl">{t('图片输入')}</span>
            <span className={'mc-sw' + (mo.img ? ' on' : '')} onClick={() => toggleModelImg(mo, refresh)}>
              <span className="track"></span><span className="slabel">{mo.img ? t('是') : t('否')}</span>
            </span>
          </div>
          <div className="erow">
            <span className="elbl">{t('别名')}</span>
            <div className="alias-list">
              {mo.alias.length ? mo.alias.map(a => (
                <span className="keytag" key={a}>
                  {a}{' '}
                  <a
                    href="#"
                    aria-label={t('删除别名')}
                    className="alias-x"
                    style={{ color: 'var(--color-feedback-error-text)', textDecoration: 'none' }}
                    onClick={e => { e.preventDefault(); delAlias(a); }}
                  ><Icon name="x" /></a>
                </span>
              )) : <span className="faint" style={{ fontSize: '0.75rem' }}>{t('无')}</span>}
              <input className="eval" ref={aliasRef} aria-label={t('添加别名')} placeholder={t('添加别名')} style={{ maxWidth: '7.5rem' }} />
              <button className="small" onClick={addAlias}>{t('添加')}</button>
            </div>
          </div>
        </div>
        <div className="row" style={{ marginTop: '1rem', gap: '0.625rem' }}>
          <button className="primary" onClick={save}>{t('保存')}</button>
          <button className="small" onClick={() => ui({ editingModel: null })}>{t('取消')}</button>
        </div>
      </div>
    </>
  );
}

/* ---------- model groups ---------- */

function ModelGroups() {
  const { state, derived, ui } = useApp();
  const { groups } = derived;
  if (state.ui.groupEdit) return <GroupEditor />;

  const newGroup = () => ui({
    groupEdit: {
      isNew: true, id: '', index: -1,
      entries: [{ pid: derived.providers[0] ? derived.providers[0].id : '', model: '' }],
    },
  });
  const editGroup = (i: number) => {
    const g = groups[i];
    if (!g) return;
    const entries = g.members.map(m => ({ pid: m.prov, model: m.model }));
    if (!entries.length) entries.push({ pid: derived.providers[0] ? derived.providers[0].id : '', model: '' });
    ui({ groupEdit: { isNew: false, id: g.name, index: i, entries } });
  };

  return (
    <div className="section">
      <div className="section-title">{t('模型组')} <span className="note">{t('同一模型跨提供商编组 · 每个模型恒属且仅属一组')}</span></div>
      <div className="card"><table className="cards">
        <tbody>
          <tr><th>{t('组名')}</th><th>{t('成员 (provider · model)')}</th><th>{t('成员数')}</th><th></th></tr>
          {groups.map((g, i) => (
            <tr key={g.name}>
              <td data-label={t('组名')}>
                <span className="keytag" style={{ borderColor: 'var(--color-action-primary-hover)', color: 'var(--color-action-primary-hover)' }}>{g.name}</span>
              </td>
              <td data-label={t('成员 (provider · model)')}>
                {g.members.map(m => (
                  <span className="keytag" key={m.prov + ':' + m.model} style={{ marginRight: '0.375rem' }}>
                    {derived.providers.find(p => p.id === m.prov)?.name || m.prov} · {m.model}
                  </span>
                ))}
              </td>
              <td data-label={t('成员数')} className="num">{g.members.length}</td>
              <td data-label={t('操作')}><button className="small" onClick={() => editGroup(i)}>{t('编辑')}</button></td>
            </tr>
          ))}
          {groups.length === 0 && <tr><td colSpan={4} className="faint">{t('暂无数据')}</td></tr>}
        </tbody>
      </table></div>
      <div className="row" style={{ marginTop: '0.75rem' }}>
        <button className="small" onClick={newGroup}>{t('+ 新建模型组')}</button>
        <span className="faint" style={{ fontSize: '0.75rem' }}>{t('多成员组在"模型优先"模式下跨提供商兜底，长上下文靠排序而非换模型')}</span>
      </div>
    </div>
  );
}

function GroupEditor() {
  const { state, derived, ui, refreshModelGroups } = useApp();
  const e = state.ui.groupEdit as GroupEdit;
  const { providers, models, groups } = derived;

  // (pid:model) already used by other groups (excl. the one being edited) and by
  // the current draft's other rows — greyed in the per-row model dropdown.
  const occupiedFor = (rowIndex: number): Set<string> => {
    const taken = new Set<string>();
    const editingName = e.isNew ? null : (groups[e.index] ? groups[e.index].name : null);
    groups.forEach(g => {
      if (g.name === editingName) return;
      (g.members || []).forEach(m => taken.add(m.prov + ':' + m.model));
    });
    (e.entries || []).forEach((en, i) => {
      if (i === rowIndex) return;
      if (en.pid && en.model) taken.add(en.pid + ':' + en.model);
    });
    return taken;
  };

  const patchEntries = (entries: GroupEdit['entries']) => ui({ groupEdit: { ...e, entries } });
  const setEntryProv = (i: number, val: string) => {
    const entries = e.entries.map((en, j) => j === i ? { pid: val, model: '' } : en);
    patchEntries(entries);
  };
  const setEntryModel = (i: number, val: string) => {
    const entries = e.entries.map((en, j) => j === i ? { ...en, model: val } : en);
    patchEntries(entries);
  };
  const addMember = () => patchEntries([...e.entries, { pid: providers[0] ? providers[0].id : '', model: '' }]);
  const removeMember = (i: number) => patchEntries(e.entries.filter((_, j) => j !== i));

  const save = async () => {
    const id = e.id.trim();
    if (!id) { toast(t('组名')); return; }
    const entries: [string, string][] = [];
    (e.entries || []).forEach(en => { if (en.pid && en.model) entries.push([en.pid, en.model]); });
    const groupsPayload = (state.data.modelGroups || []).map(g => ({ id: g.id, entries: (g.entries || []).map(x => [x[0], x[1]] as [string, string]) }));
    if (e.isNew) {
      if (groupsPayload.some(g => g.id === id)) { toast(t('失败: ') + 'duplicate'); return; }
      groupsPayload.push({ id, entries });
    } else {
      groupsPayload[e.index] = { id, entries };
    }
    try {
      await api('PUT', '/api/model-groups', { groups: groupsPayload });
      await refreshModelGroups();
      ui({ groupEdit: null });
      toast(t('已保存'));
    } catch (err) { console.error('saveGroup', err); toast(t('失败: ') + (err as Error).message); }
  };

  const del = async () => {
    if (!confirm(t('删除该模型组？'))) return;
    const groupsPayload = (state.data.modelGroups || []).map(g => ({ id: g.id, entries: (g.entries || []).map(x => [x[0], x[1]] as [string, string]) }));
    groupsPayload.splice(e.index, 1);
    try {
      await api('PUT', '/api/model-groups', { groups: groupsPayload });
      await refreshModelGroups();
      ui({ groupEdit: null });
      toast(t('已删除'));
    } catch (err) { console.error('delGroup', err); toast(t('失败: ') + (err as Error).message); }
  };

  return (
    <>
      <div className="edit-head"><button className="ghost" onClick={() => ui({ groupEdit: null })}>{t('← 返回')}</button></div>
      <div className="edit-card grp-wide">
        <div className="edit-name grp-sans">{e.id || t('+ 新建模型组')}</div>
        <div className="card card-pad">
          <div className="pd-field">
            <span className="lbl">{t('组名')}</span>
            <input
              className="val"
              aria-label={t('组名')}
              value={e.id}
              readOnly={!e.isNew}
              onChange={ev => ui({ groupEdit: { ...e, id: ev.target.value } })}
            />
          </div>
        </div>
        <div className="card card-pad">
          <div className="section-title">{t('成员')}</div>
          <div className="grp-members">
            {e.entries.map((en, i) => {
              const pid = en.pid || (providers[0] ? providers[0].id : '');
              const provModels = models.filter(m => m.prov === pid);
              const taken = occupiedFor(i);
              const has = provModels.length > 0;
              const cur = en.model || '';
              const curInList = provModels.some(m => m.id === cur);
              return (
                <div className="grp-member" key={i}>
                  <select className="gp-prov" aria-label={t('provider')} value={pid} onChange={ev => setEntryProv(i, ev.target.value)}>
                    {providers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
                  </select>
                  <select className="gp-model mono" aria-label={t('upstream model')} value={cur} disabled={!has} onChange={ev => setEntryModel(i, ev.target.value)}>
                    {!has && <option value="" disabled>{t('暂无可用模型')}</option>}
                    {has && !cur && <option value="" disabled>{t('选择模型')}</option>}
                    {cur && !curInList && <option value={cur}>{cur}</option>}
                    {provModels.map(m => {
                      const occ = taken.has(pid + ':' + m.id);
                      return <option key={m.id} value={m.id} disabled={occ}>{m.id}{occ ? ' · ' + t('已编组') : ''}</option>;
                    })}
                  </select>
                  <button
                    className="small"
                    style={{ borderColor: 'var(--color-feedback-error-text)', color: 'var(--color-feedback-error-text)' }}
                    onClick={() => removeMember(i)}
                  >{t('删除')}</button>
                </div>
              );
            })}
          </div>
          <div className="row" style={{ marginTop: '0.75rem' }}>
            <button className="small" onClick={addMember}>{t('添加成员')}</button>
          </div>
        </div>
        <div className="row" style={{ marginTop: '1.5rem', gap: '0.625rem' }}>
          <button className="primary" onClick={save}>{t('保存')}</button>
          <button className="small" onClick={() => ui({ groupEdit: null })}>{t('取消')}</button>
          {!e.isNew && (
            <button
              className="small"
              style={{ borderColor: 'var(--color-feedback-error-text)', color: 'var(--color-feedback-error-text)' }}
              onClick={del}
            >{t('删除')}</button>
          )}
        </div>
      </div>
    </>
  );
}

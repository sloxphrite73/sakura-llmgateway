/* Strategy view — filters (2 toggles) + sort stack (3 selects) + dry-run board.
   Port of renderStrategy() / resultBanner() / onFilterChange() / onSortChange(). */

import React from 'react';
import { t, tp } from '../i18n';
import { useApp } from '../store';
import type { RouteResult } from '../types';

/* sort comparators (A-L) — display names only; the server applies them. */
const SORTS: Record<string, string> = {
  A: '成功率 ↓',
  B: 'RPM ↓',
  C: 'TPM ↓',
  D: '首字延迟 ↑',
  E: 'token余额 ↓',
  F: 'token余额 ↑',
  G: '账单余额 ↓',
  H: '账单余额 ↑',
  I: '单价 ↑',
  J: '生成速度TPS ↓',
  K: '可用性 ↓',
  L: 'TPM余量 ↓',
};
const SORT_KEYS = Object.keys(SORTS);

export function StrategyView() {
  const { state, derived, ui, putStrategy, runDryRun, pname } = useApp();
  const { models, providers } = derived;
  const { groupLock, provLock, sorts, reqModel, reqProv } = state.ui;
  const res = state.data.res || { state: '429-empty' as const, winner: null, cand: [], retry: null };
  const modelIds = [...new Set(models.map(mo => mo.id))];

  const modeShort = () => {
    if (groupLock && provLock) return t('精确轮换');
    if (groupLock && !provLock) return t('模型优先');
    if (!groupLock && provLock) return t('提供商优先');
    return t('可用优先');
  };

  // Overrides are passed explicitly because the store update (dispatch) only
  // becomes visible to stateRef on the next render — the PUT/dry-run below must
  // use the new values immediately (same call order as the original: PUT → dry-run).
  const onFilterChange = async (patch: { groupLock?: boolean; provLock?: boolean }) => {
    ui(patch);
    await putStrategy(patch);
    await runDryRun();
  };
  const onSortChange = async (i: number, val: string) => {
    const next = sorts.slice();
    next[i] = val;
    ui({ sorts: next });
    await putStrategy({ sorts: next });
    await runDryRun({ sorts: next });
  };
  const onReqModelChange = async (val: string) => { ui({ reqModel: val }); await runDryRun({ model: val }); };
  const onReqProvChange = async (val: string) => { ui({ reqProv: val }); await runDryRun({ prov: val }); };

  return (
    <>
      <h2 className="board-title">{t('策略管理')}</h2>
      <div className="section">
        <div className="section-title">{t('过滤器')}</div>
        <div className="card card-pad">
          <div className="toggle-row">
            <span className="tlbl">{t('不改变模型组')}</span>
            <label className="toggle">
              <input type="checkbox" aria-label={t('不改变模型组')} checked={groupLock} onChange={e => onFilterChange({ groupLock: e.target.checked })} />
              <span className="track"></span>
            </label>
          </div>
          <div className="toggle-row">
            <span className="tlbl">{t('不改变提供商')}</span>
            <label className="toggle">
              <input type="checkbox" aria-label={t('不改变提供商')} checked={provLock} onChange={e => onFilterChange({ provLock: e.target.checked })} />
              <span className="track"></span>
            </label>
          </div>
          <div className="faint" style={{ fontSize: '0.75rem', marginTop: '0.25rem' }}>{t('当前：')}{modeShort()}</div>
        </div>
      </div>
      <div className="section">
        <div className="section-title">{t('优先策略')} <span className="note">{t('关键字 3 / 4 / 5（1=valid↓ 2=非冷却↓ 固定前导）')}</span></div>
        <div className="card card-pad">
          {[0, 1, 2].map(i => (
            <div className="prio-row" key={i}>
              <span className="plbl">{tp('关键字 {n}', { n: i + 3 })}</span>
              <select aria-label={tp('关键字 {n}', { n: i + 3 })} value={sorts[i]} onChange={e => onSortChange(i, e.target.value)}>
                <option value="">{t('（跳过）')}</option>
                {SORT_KEYS.map(s => <option key={s} value={s}>{s} · {t(SORTS[s])}</option>)}
              </select>
            </div>
          ))}
        </div>
      </div>
      <div className="section">
        <div className="section-title">{t('演示')} <span className="note">{t('dry-run 实时')}</span></div>
        <div className="mf-row" style={{ margin: '0 0 0.875rem' }}>
          <div className="mf-left">
            <span className="faint" style={{ fontSize: '0.75rem' }}>{t('模拟请求')}</span>
            <select aria-label={t('请求模型')} value={reqModel} onChange={e => onReqModelChange(e.target.value)}>
              {modelIds.map(id => <option key={id} value={id}>{id}</option>)}
            </select>
            <span className="faint">@</span>
            <select aria-label={t('请求提供商')} value={reqProv} onChange={e => onReqProvChange(e.target.value)}>
              {providers.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
            </select>
          </div>
        </div>
        <ResultBanner res={res} />
        <div className="card"><table className="cards">
          <tbody>
            <tr><th>{t('#')}</th><th>{t('Key')}</th><th>{t('提供商')}</th><th>{t('模型')}</th><th>{t('valid')}</th><th>{t('cool')}</th><th>{t('单价')}</th><th>{t('TPS')}</th><th>{t('成功率')}</th></tr>
            {res.cand.map((k, i) => (
              <tr key={k.kid + ':' + k.model + ':' + i} className={res.winner === k ? 'row-winner' : ''}>
                <td data-label="#" className="num faint">{i + 1}</td>
                <td data-label={t('Key')}><span className="keytag">{k.kid}</span></td>
                <td data-label={t('提供商')}>{pname(k.prov)}</td>
                <td data-label={t('模型')}><span className="keytag">{k.model}</span></td>
                <td data-label={t('valid')}>{k.valid ? <span className="st ok">{t('是')}</span> : <span className="st off">{t('否')}</span>}</td>
                <td data-label={t('cool')}>{k.cool ? <span className="st cool">{k.cd}s</span> : <span className="faint">-</span>}</td>
                <td data-label={t('单价')} className="num">{k.price.toFixed(4)}</td>
                <td data-label={t('TPS')} className="num">{k.tps}</td>
                <td data-label={t('成功率')} className="num">{(k.sr * 100).toFixed(1)}%</td>
              </tr>
            ))}
            {res.cand.length === 0 && <tr><td colSpan={9} className="faint">{t('候选为空')}</td></tr>}
          </tbody>
        </table></div>
      </div>
    </>
  );
}

function ResultBanner({ res }: { res: RouteResult }) {
  const { pname } = useApp();
  if (res.state === 'routed' && res.winner) {
    return (
      <div className="result-banner routed">
        <span className="big">{t('路由命中')}</span>{' '}
        <span className="keytag">{res.winner.kid}</span> · {pname(res.winner.prov)} / {res.winner.model}
      </div>
    );
  }
  if (res.state === '429-retry') {
    return (
      <div className="result-banner err429">
        <span className="big">{t('429 全冷却')}</span> {t('返回')} <code>Retry-After: {res.retry != null ? res.retry + 's' : '-'}</code>
      </div>
    );
  }
  if (res.state === '429-novalid') {
    return (
      <div className="result-banner err429">
        <span className="big">{t('429 无可用')}</span> {tp('候选 {n} 条均无效', { n: res.cand.length })}
      </div>
    );
  }
  return (
    <div className="result-banner err429">
      <span className="big">{t('429 候选为空')}</span> {t('当前过滤下无候选')}
    </div>
  );
}

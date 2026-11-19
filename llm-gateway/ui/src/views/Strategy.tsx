/* Strategy view — preset cards + dry-run board.
   Manual strategy editing (filter toggles + sort-key selects) was removed:
   routing-strategy changes now go through gateway.json (or a config import).
   The preset grid is the future one-click apply surface; preset definitions
   are not open yet, so it renders two disabled placeholder slots until the
   definition mechanism lands. */

import React from 'react';
import { t, tp } from '../i18n';
import { useApp } from '../store';
import type { RouteResult } from '../types';

/* Strategy presets — one card applies a whole strategy (filter + sort stack).
   Empty for now: the definition mechanism (a gateway.json field + an apply
   endpoint) lands later; with zero presets the grid shows two disabled
   placeholder slots. */
const PRESETS: { name: string; sorts: string[] }[] = [];

export function StrategyView() {
  const { state, derived, ui, runDryRun, pname } = useApp();
  const { models, providers } = derived;
  const { reqModel, reqProv } = state.ui;
  const res = state.data.res || { state: '429-empty' as const, winner: null, cand: [], retry: null };
  const modelIds = [...new Set(models.map(mo => mo.id))];

  const onReqModelChange = async (val: string) => { ui({ reqModel: val }); await runDryRun({ model: val }); };
  const onReqProvChange = async (val: string) => { ui({ reqProv: val }); await runDryRun({ prov: val }); };

  return (
    <>
      <h2 className="board-title">{t('策略管理')}</h2>
      <div className="section">
        <div className="section-title">{t('策略预设')} <span className="note">{t('一键应用整套路由策略（过滤器 + 排序栈）')}</span></div>
        <div className="preset-grid">
          {PRESETS.length > 0 ? PRESETS.map(p => (
            <button type="button" key={p.name} className="preset-card" aria-label={p.name}>
              <span className="pname">{p.name}</span>
              <span className="faint" style={{ fontSize: '0.75rem' }}>{p.sorts.join(' → ')}</span>
            </button>
          )) : [0, 1].map(i => (
            <div key={i} className="preset-card is-empty" aria-disabled="true">
              <span className="pname">{t('未配置预设')}</span>
              <span className="faint" style={{ fontSize: '0.75rem' }}>{t('预设定义机制将在后续版本开放')}</span>
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

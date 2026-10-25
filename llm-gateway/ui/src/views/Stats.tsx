/* Stats view — port of renderStats(). Data from /api/stats (1s change-detected
   poll in the store). Histogram bars clickable → inline detail strip. */

import React from 'react';
import { t, tp } from '../i18n';
import { useApp } from '../store';
import type { StatsHour, StatEntry } from '../types';

function hh(h: StatsHour): number {
  try { return new Date(h.hour_ms).getHours(); } catch (_) { return 0; }
}

export function StatsView() {
  const { state, ui } = useApp();
  const stats = state.data.stats;
  const histSel = state.ui.histSel;

  const tot = (stats && stats.total) || { success: 0, fail: 0, tokens: 0 };
  const rot = (stats && stats.rotations_total) || 0;
  const allreq = tot.success + tot.fail;
  const rate = allreq ? (tot.success / allreq * 100).toFixed(1) + '%' : '-';
  const totTok = tot.tokens || 0;
  // Token 总消耗: ≤1000 完整显示；>1000 截断末三位按 K 显示（1,234,567 → "1,234K"），
  // 完整值放原生 title。
  const totTokDisp = totTok <= 1000 ? totTok.toLocaleString() : Math.floor(totTok / 1000).toLocaleString() + 'K';
  const hist = (stats && stats.histogram) || [];
  const peakReq = Math.max(1, ...hist.map(h => h.success + h.fail));
  const peakTok = Math.max(1, ...hist.map(h => h.tokens || 0));
  const provStats = Object.entries((stats && stats.providers) || {});
  const modelStats = Object.entries((stats && stats.models) || {});
  const keyStats = Object.entries((stats && stats.keys) || {});

  const statRows = (entries: [string, StatEntry][], cols: number, render: (e: [string, StatEntry]) => React.ReactNode) =>
    entries.length
      ? entries.map(render)
      : <tr><td colSpan={cols} className="faint">{t('暂无数据')}</td></tr>;

  const toggleHistSel = (chart: 'req' | 'tok', i: number) => {
    ui({ histSel: histSel && histSel.chart === chart && histSel.i === i ? null : { chart, i } });
  };

  const histDetail = (chart: 'req' | 'tok') => {
    if (!histSel || histSel.chart !== chart) return null;
    const h = hist[histSel.i];
    if (!h) return null;
    return (
      <div className="hist-detail">
        {chart === 'req'
          ? <>{hh(h)}:00 · {t('成功')} <b>{h.success}</b> / {t('失败')} <b>{h.fail}</b> · <b>{(h.tokens || 0).toLocaleString()}</b> tok</>
          : <>{hh(h)}:00 · <b>{(h.tokens || 0).toLocaleString()}</b> tok</>}
      </div>
    );
  };

  return (
    <>
      <h2 className="board-title">{t('统计')}</h2>
      <div className="board-note">{t('请求与 Token 用量 · 数据持久化，重启不丢失')}</div>
      <div className="section">
        <div className="section-title">{t('总览')}</div>
        <div className="stat-grid">
          <div className="stat-box ok"><div className="num">{tot.success.toLocaleString()}</div><div className="label">{t('成功请求')}</div></div>
          <div className="stat-box err"><div className="num">{tot.fail.toLocaleString()}</div><div className="label">{t('失败请求')}</div></div>
          <div className="stat-box warn"><div className="num">{rot}</div><div className="label">{t('换 Key 次数')}</div></div>
          <div className="stat-box"><div className="num">{rate}</div><div className="label">{t('成功率')}</div></div>
          <div className="stat-box"><div className="num" title={totTok.toLocaleString()}>{totTokDisp}</div><div className="label">{t('Token 总消耗')}</div></div>
        </div>
      </div>
      <div className="section">
        <div className="section-title">{t('过去 24 小时请求')} <span className="note">{t('每小时一柱 · 绿=成功 红=失败')}</span></div>
        <div className="card card-pad">
          {hist.length ? (
            <>
              <div className="hist-scroll">
                <div className="hist">
                  {hist.map((h, i) => {
                    const okH = (h.success / peakReq * 100).toFixed(2);
                    const failH = (h.fail / peakReq * 100).toFixed(2);
                    const sel = histSel && histSel.chart === 'req' && histSel.i === i;
                    return (
                      <div
                        key={i}
                        className={'bar-wrap' + (sel ? ' sel' : '')}
                        title={tp('{h}:00 · 成功 {ok} / 失败 {fail}', { h: hh(h), ok: h.success, fail: h.fail })}
                        onClick={() => toggleHistSel('req', i)}
                      >
                        {h.fail ? <div className="bar fail" style={{ height: failH + '%' }}></div> : null}
                        {h.success ? <div className="bar ok" style={{ height: okH + '%' }}></div> : null}
                      </div>
                    );
                  })}
                </div>
                <div className="hist-x">
                  {hist.map((h, i) => <span key={i}>{i % 4 === 0 ? hh(h) + ':00' : ''}</span>)}
                </div>
              </div>
              {histDetail('req')}
            </>
          ) : <div className="empty">{t('暂无数据')}</div>}
        </div>
      </div>
      <div className="section">
        <div className="section-title">{t('过去 24 小时 Token 用量')} <span className="note">{t('每小时一柱')}</span></div>
        <div className="card card-pad">
          {hist.length ? (
            <>
              <div className="hist-scroll">
                <div className="hist">
                  {hist.map((h, i) => {
                    const tH = (((h.tokens || 0) / peakTok) * 100).toFixed(2);
                    const sel = histSel && histSel.chart === 'tok' && histSel.i === i;
                    return (
                      <div
                        key={i}
                        className={'bar-wrap' + (sel ? ' sel' : '')}
                        title={tp('{h}:00 · {n} tok', { h: hh(h), n: (h.tokens || 0).toLocaleString() })}
                        onClick={() => toggleHistSel('tok', i)}
                      >
                        {(h.tokens || 0) ? <div className="bar tok" style={{ height: tH + '%' }}></div> : null}
                      </div>
                    );
                  })}
                </div>
                <div className="hist-x">
                  {hist.map((h, i) => <span key={i}>{i % 4 === 0 ? hh(h) + ':00' : ''}</span>)}
                </div>
              </div>
              {histDetail('tok')}
            </>
          ) : <div className="empty">{t('暂无数据')}</div>}
        </div>
      </div>
      <div className="section">
        <div className="section-title">{t('按提供商')}</div>
        <div className="card"><table className="cards">
          <tbody>
            <tr><th>{t('提供商')}</th><th>{t('请求')}</th><th>{t('成功')}</th><th>{t('失败')}</th><th>{t('Token 消耗')}</th></tr>
            {statRows(provStats, 5, ([id, p]) => {
              const req = p.success + p.fail;
              return (
                <tr key={id}>
                  <td data-label={t('提供商')}>{p.name}</td>
                  <td data-label={t('请求')} className="num">{req.toLocaleString()}</td>
                  <td data-label={t('成功')} className="num" style={{ color: 'var(--color-feedback-success-text)' }}>{p.success.toLocaleString()}</td>
                  <td data-label={t('失败')} className="num" style={{ color: 'var(--color-feedback-error-text)' }}>{p.fail}</td>
                  <td data-label={t('Token 消耗')} className="num">{(p.tokens || 0).toLocaleString()}</td>
                </tr>
              );
            })}
          </tbody>
        </table></div>
      </div>
      <div className="section">
        <div className="section-title">{t('按模型')}</div>
        <div className="card"><table className="cards">
          <tbody>
            <tr><th>{t('模型')}</th><th>{t('提供商')}</th><th>{t('请求')}</th><th>{t('成功')}</th><th>{t('失败')}</th><th>{t('Token 消耗')}</th></tr>
            {statRows(modelStats, 6, ([mid, s]) => {
              const req = s.success + s.fail;
              return (
                <tr key={mid}>
                  <td data-label={t('模型')}><span className="keytag">{mid}</span></td>
                  <td data-label={t('提供商')} className="faint">-</td>
                  <td data-label={t('请求')} className="num">{req.toLocaleString()}</td>
                  <td data-label={t('成功')} className="num" style={{ color: 'var(--color-feedback-success-text)' }}>{s.success.toLocaleString()}</td>
                  <td data-label={t('失败')} className="num" style={{ color: 'var(--color-feedback-error-text)' }}>{s.fail}</td>
                  <td data-label={t('Token 消耗')} className="num">{(s.tokens || 0).toLocaleString()}</td>
                </tr>
              );
            })}
          </tbody>
        </table></div>
      </div>
      <div className="section">
        <div className="section-title">{t('按 API Key')}</div>
        <div className="card"><table className="cards">
          <tbody>
            <tr><th>{t('API Key')}</th><th>{t('请求')}</th><th>{t('成功')}</th><th>{t('失败')}</th><th>{t('Token 消耗')}</th></tr>
            {statRows(keyStats, 5, ([id, k]) => {
              const req = k.success + k.fail;
              return (
                <tr key={id}>
                  <td data-label={t('API Key')}>{k.name}</td>
                  <td data-label={t('请求')} className="num">{req.toLocaleString()}</td>
                  <td data-label={t('成功')} className="num" style={{ color: 'var(--color-feedback-success-text)' }}>{k.success.toLocaleString()}</td>
                  <td data-label={t('失败')} className="num" style={{ color: 'var(--color-feedback-error-text)' }}>{k.fail}</td>
                  <td data-label={t('Token 消耗')} className="num">{(k.tokens || 0).toLocaleString()}</td>
                </tr>
              );
            })}
          </tbody>
        </table></div>
      </div>
    </>
  );
}

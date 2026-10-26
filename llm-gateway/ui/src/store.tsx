/* Global state: one useReducer store + a controller with the async actions.
   Replaces the single-file console's global S + PROVIDERS/MODELS/KEYS/GROUPS +
   STATUS/STATS/STRATEGY/MODEL_GROUPS/DRY/RES. */

import React, {
  createContext, useCallback, useContext, useEffect, useMemo,
  useReducer, useRef,
} from 'react';
import { api, apiText } from './api';
import { adaptAll, type Adapted } from './adapt';
import { setLang as setI18nLang, initLang, type Lang } from './i18n';
import { applyTheme, DEFAULT_THEME_ID, type ThemeMeta } from './theme';
import { toast } from './toast';
import { t } from './i18n';
import type {
  CatalogEntry, DryRunResult, ModelGroup, RouteResult, Stats, Status, Strategy,
} from './types';

export type TabId = 'stats' | 'providers' | 'models' | 'strategy' | 'apikeys' | 'settings';
export type AddState = null | 'choose' | 'catalog' | 'form';
export interface AddDraft {
  name: string; base_url: string; protocol: string;
  models: string[]; key: string; source: 'custom' | 'catalog';
}
export interface GroupEditEntry { pid: string; model: string; }
export interface GroupEdit {
  isNew: boolean; id: string; index: number; entries: GroupEditEntry[];
}
export interface HistSel { chart: 'req' | 'tok'; i: number; }

export interface UiState {
  tab: TabId;
  lang: Lang;
  collapsed: boolean;
  localProv: string | null;
  modelTab: 'info' | 'groups';
  modelFilter: string;
  editingModel: number | null;
  groupEdit: GroupEdit | null;
  keyFilter: string;
  reqModel: string;
  reqProv: string;
  groupLock: boolean;
  provLock: boolean;
  sorts: string[];
  addState: AddState;
  addDraft: AddDraft | null;
  akEdit: string | null;
  akEditKey: string | null;
  recycled: number;
  histSel: HistSel | null;
}

export interface DataState {
  status: Status | null;
  stats: Stats | null;
  catalog: CatalogEntry[];
  strategy: Strategy | null;
  modelGroups: ModelGroup[];
  res: RouteResult | null;
  /** available themes from /api/themes + the selected theme id */
  themes: ThemeMeta[];
  themeId: string;
  /** effective server now (ms), ticked 1/s locally off the /api/status clock */
  now: number;
}

export interface AppState { ui: UiState; data: DataState; }

type Action =
  | { type: 'ui'; patch: Partial<UiState> }
  | { type: 'data'; patch: Partial<DataState> };

function reducer(s: AppState, a: Action): AppState {
  if (a.type === 'ui') return { ...s, ui: { ...s.ui, ...a.patch } };
  return { ...s, data: { ...s.data, ...a.patch } };
}

/* ============ UI state persistence (survive Android rotation reloads) ============ */
const UI_STATE_KEYS = [
  'tab', 'addState', 'editingModel', 'groupEdit', 'keyFilter',
  'modelFilter', 'modelTab', 'reqModel', 'reqProv',
] as const;

function saveUiState(ui: UiState): void {
  try {
    const o: Record<string, unknown> = {};
    for (const k of UI_STATE_KEYS) o[k] = ui[k as keyof UiState];
    if (ui.addDraft) o.addDraft = ui.addDraft;
    if (ui.groupEdit) o.groupEdit = ui.groupEdit;
    localStorage.setItem('uiState', JSON.stringify(o));
  } catch (_) { /* ignore */ }
}

function restoreUiState(): Partial<UiState> {
  try {
    const o = JSON.parse(localStorage.getItem('uiState') || 'null');
    if (!o) return {};
    const out: Record<string, unknown> = {};
    for (const k of UI_STATE_KEYS) if (o[k] !== undefined && o[k] !== null) out[k] = o[k];
    if (o.addDraft) out.addDraft = o.addDraft;
    if (o.groupEdit) out.groupEdit = o.groupEdit;
    return out as Partial<UiState>;
  } catch (_) { return {}; }
}

function initialState(): AppState {
  const lang = initLang();
  return {
    ui: {
      tab: 'stats',
      lang,
      collapsed: false,
      localProv: null,
      modelTab: 'info',
      modelFilter: 'all',
      editingModel: null,
      groupEdit: null,
      keyFilter: 'all',
      reqModel: '',
      reqProv: '',
      groupLock: true,
      provLock: true,
      sorts: ['', '', ''],
      addState: null,
      addDraft: null,
      akEdit: null,
      akEditKey: null,
      recycled: 0,
      histSel: null,
      ...restoreUiState(),
    },
    data: {
      status: null, stats: null, catalog: [], strategy: null,
      modelGroups: [], res: null, themes: [], themeId: DEFAULT_THEME_ID, now: Date.now(),
    },
  };
}

export interface Controller {
  state: AppState;
  derived: Adapted;
  dispatch: React.Dispatch<Action>;
  ui: (patch: Partial<UiState>) => void;
  setData: (patch: Partial<DataState>) => void;
  setTab: (id: TabId) => void;
  setLocalProv: (id: string) => void;
  setModelTab: (mt: 'info' | 'groups') => void;
  setLang: (l: Lang) => void;
  pname: (id: string) => string;
  refresh: () => Promise<void>;
  refreshAkStatus: () => Promise<void>;
  runDryRun: (override?: { model?: string; prov?: string; sorts?: string[] }) => Promise<void>;
  putStrategy: (override?: { groupLock?: boolean; provLock?: boolean; sorts?: string[] }) => Promise<void>;
  findRealKey: (keyId: string) => Promise<string | null>;
  refreshModelGroups: () => Promise<void>;
  /** Switch console theme: persist to gateway settings + apply immediately. */
  setTheme: (id: string) => Promise<void>;
}

const Ctx = createContext<Controller | null>(null);
export function useApp(): Controller {
  const c = useContext(Ctx);
  if (!c) throw new Error('useApp outside provider');
  return c;
}

export function AppProvider({ children }: { children: React.ReactNode }) {
  const [state, dispatch] = useReducer(reducer, undefined, initialState);
  const stateRef = useRef(state);
  stateRef.current = state;
  // server clock offset: effectiveNow = Date.now() + offset (same machine in
  // practice — desktop exe or on-device Android — so this is ≈0, but keeps the
  // countdown math anchored to the server's now_ms like the original).
  const offsetRef = useRef(0);
  const akSyncingRef = useRef(false);
  const expiredRef = useRef<Set<string>>(new Set());
  const lastStatsJsonRef = useRef('');

  const ui = useCallback((patch: Partial<UiState>) => dispatch({ type: 'ui', patch }), []);
  const setData = useCallback((patch: Partial<DataState>) => dispatch({ type: 'data', patch }), []);

  const derived = useMemo(
    () => adaptAll(state.data.status, state.data.stats, state.data.modelGroups),
    [state.data.status, state.data.stats, state.data.modelGroups],
  );
  const derivedRef = useRef(derived);
  derivedRef.current = derived;

  const pname = useCallback(
    (id: string) => (derivedRef.current.providers.find(p => p.id === id) || {}).name || id,
    [],
  );

  /* ---------- dry-run (computeRoute replacement) ---------- */
  const runDryRun = useCallback(async (override?: { model?: string; prov?: string; sorts?: string[] }) => {
    const { ui } = stateRef.current;
    const model = override && override.model !== undefined ? override.model : ui.reqModel;
    const prov = override && override.prov !== undefined ? override.prov : ui.reqProv;
    const sorts = override && override.sorts ? override.sorts : ui.sorts;
    if (!model) return;
    try {
      const d = await api<DryRunResult>('POST', '/api/strategy/dry-run', {
        model,
        provider: prov || undefined,
        sorts: sorts.filter(Boolean),
      });
      const cand = (d.candidates || []).map(k => ({
        routed: k.routed,
        kid: k.api_key_id, prov: k.provider_id, model: k.upstream_model,
        valid: k.valid, cool: !k.non_cooled, cd: k.remaining_secs || 0,
        price: k.price || 0, tps: k.tps || 0, sr: k.success_rate || 0,
        avail: k.availability != null ? k.availability : 1,
      }));
      let winner = null;
      for (const k of cand) if (k.routed) winner = k;
      let state2: RouteResult['state']; let retry: number | null | undefined;
      if (d.result === 'routed') state2 = 'routed';
      else if (d.result === 'all_cooling') { state2 = '429-retry'; retry = d.retry_after; }
      else if (d.result === 'no_valid') state2 = '429-novalid';
      else state2 = '429-empty';
      dispatch({ type: 'data', patch: { res: { state: state2, winner, cand, retry } } });
    } catch (e) {
      dispatch({ type: 'data', patch: { res: null } });
      console.error('runDryRun', e);
    }
  }, []);
  const runDryRunRef = useRef(runDryRun);
  runDryRunRef.current = runDryRun;

  /* ---------- full refresh ---------- */
  const refresh = useCallback(async () => {
    try {
      const status = await api<Status>('GET', '/api/status');
      const stats = await api<Stats>('GET', '/api/stats');
      const cat = await api<{ providers?: CatalogEntry[] }>('GET', '/api/free-catalog');
      const strategy = await api<Strategy>('GET', '/api/strategy');
      const modelGroups = (await api<ModelGroup[]>('GET', '/api/model-groups')) || [];
      let themes: ThemeMeta[] = [];
      let themeId = stateRef.current.data.themeId;
      try {
        const th = await api<{ current?: string; themes?: ThemeMeta[] }>('GET', '/api/themes');
        themes = th.themes || [];
        if (th.current) themeId = th.current;
      } catch (e) { /* older gateway without theme support */ }
      offsetRef.current = (status.now_ms != null ? status.now_ms : Date.now()) - Date.now();
      try { lastStatsJsonRef.current = JSON.stringify(stats); } catch (_) { /* ignore */ }
      dispatch({
        type: 'data',
        patch: { status, stats, catalog: (cat && cat.providers) || [], strategy, modelGroups, themes, themeId },
      });
      // mirror the saved strategy into the UI state (edits PUT immediately)
      const u = stateRef.current.ui;
      const patch: Partial<UiState> = {
        groupLock: strategy.filter.lock_model_group,
        provLock: strategy.filter.lock_provider,
        sorts: [strategy.sort[0] || '', strategy.sort[1] || '', strategy.sort[2] || ''],
      };
      const d2 = adaptAll(status, stats, modelGroups);
      if (!u.reqModel && d2.models.length) patch.reqModel = d2.models[0].id;
      if (!u.reqProv && d2.providers.length) patch.reqProv = d2.providers[0].id;
      dispatch({ type: 'ui', patch });
      // stateRef only updates after render — pass the freshly chosen values
      // explicitly so the very first dry-run isn't skipped (empty reqModel).
      await runDryRunRef.current({
        model: patch.reqModel !== undefined ? patch.reqModel : u.reqModel,
        prov: patch.reqProv !== undefined ? patch.reqProv : u.reqProv,
        sorts: patch.sorts,
      });
    } catch (e) { /* transient; next tick retries */ }
  }, []);

  /* ---------- api key live status (1s poll) ---------- */
  const refreshAkStatus = useCallback(async () => {
    if (document.hidden) return;
    try {
      const status = await api<Status>('GET', '/api/status');
      offsetRef.current = (status.now_ms != null ? status.now_ms : Date.now()) - Date.now();
      expiredRef.current.clear();
      dispatch({ type: 'data', patch: { status } });
    } catch (e) { /* transient; next tick retries */ }
  }, []);

  /* ---------- stats live refresh (1s poll, change-detected) ---------- */
  const refreshStats = useCallback(async () => {
    if (stateRef.current.ui.tab !== 'stats' || document.hidden) return;
    try {
      const raw = await apiText('GET', '/api/stats');
      if (raw === lastStatsJsonRef.current) return; // no change → no render
      lastStatsJsonRef.current = raw;
      dispatch({ type: 'data', patch: { stats: JSON.parse(raw) } });
    } catch (e) { /* transient; next tick retries */ }
  }, []);

  /* ---------- 1s local tick: smooth countdown, expiry re-sync ---------- */
  const tick = useCallback(() => {
    const now = Date.now() + offsetRef.current;
    dispatch({ type: 'data', patch: { now } });
    if (stateRef.current.ui.tab !== 'apikeys' || document.hidden) return;
    let anyExpired = false;
    for (const k of derivedRef.current.keys) {
      if (!k.cool || !k.untilMs) continue;
      if (k.untilMs <= now && !expiredRef.current.has(k.kid)) {
        expiredRef.current.add(k.kid);
        anyExpired = true;
      }
    }
    if (anyExpired && !akSyncingRef.current) {
      akSyncingRef.current = true;
      refreshAkStatus().finally(() => { akSyncingRef.current = false; });
    }
  }, [refreshAkStatus]);

  /* ---------- strategy PUT ---------- */
  const putStrategy = useCallback(async (override?: { groupLock?: boolean; provLock?: boolean; sorts?: string[] }) => {
    const { ui } = stateRef.current;
    const groupLock = override && override.groupLock !== undefined ? override.groupLock : ui.groupLock;
    const provLock = override && override.provLock !== undefined ? override.provLock : ui.provLock;
    const sorts = override && override.sorts ? override.sorts : ui.sorts;
    try {
      await api('PUT', '/api/strategy', {
        filter: { lock_model_group: groupLock, lock_provider: provLock },
        sort: sorts.filter(Boolean),
      });
      const strategy = await api<Strategy>('GET', '/api/strategy');
      dispatch({ type: 'data', patch: { strategy } });
      dispatch({
        type: 'ui',
        patch: {
          groupLock: strategy.filter.lock_model_group,
          provLock: strategy.filter.lock_provider,
          sorts: [strategy.sort[0] || '', strategy.sort[1] || '', strategy.sort[2] || ''],
        },
      });
    } catch (e) {
      console.error('putStrategy', e);
      toast(t('失败: ') + (e as Error).message);
    }
  }, []);

  /* ---------- misc actions ---------- */
  const setTab = useCallback((id: TabId) => {
    const patch: Partial<UiState> = { tab: id };
    if (id !== 'models') patch.editingModel = null;
    dispatch({ type: 'ui', patch });
    if (id === 'apikeys') refreshAkStatus();
  }, [refreshAkStatus]);

  const setLocalProv = useCallback((id: string) => {
    dispatch({ type: 'ui', patch: { localProv: id, addState: null } });
  }, []);

  const setModelTab = useCallback((mt: 'info' | 'groups') => {
    dispatch({ type: 'ui', patch: { modelTab: mt, editingModel: null, groupEdit: null } });
  }, []);

  const setLang = useCallback((l: Lang) => {
    setI18nLang(l);
    dispatch({ type: 'ui', patch: { lang: l } });
  }, []);

  const findRealKey = useCallback(async (keyId: string): Promise<string | null> => {
    const cfg = await api<{ providers?: { keys?: { id: string; key: string }[] }[] }>('GET', '/api/config/export');
    for (const p of (cfg.providers || [])) for (const k of (p.keys || [])) if (k.id === keyId) return k.key;
    return null;
  }, []);

  const refreshModelGroups = useCallback(async () => {
    const modelGroups = (await api<ModelGroup[]>('GET', '/api/model-groups')) || [];
    dispatch({ type: 'data', patch: { modelGroups } });
  }, []);

  const setTheme = useCallback(async (id: string) => {
    const d = stateRef.current.data;
    const s = d.status;
    if (!s) return;
    try {
      // SettingsInput is a full-settings write — resend current values + theme.
      await api('PUT', '/api/settings', {
        default_cooldown_secs: s.default_cooldown_secs ?? 60,
        max_attempts: s.max_attempts ?? 3,
        max_hold_secs: s.max_hold_secs ?? 120,
        auth: s.auth || { enabled: false, keys: [] },
        api_port: s.api_port || 8000,
        ui_port: s.ui_port || 8001,
        theme: id,
      });
      dispatch({ type: 'data', patch: { themeId: id } });
    } catch (e) {
      console.error('setTheme', e);
      toast(t('失败: ') + (e as Error).message);
    }
  }, []);

  // Apply whenever the selected theme id or the manifests arrive/change.
  useEffect(() => {
    const { themes, themeId } = state.data;
    const meta = themes.find(th => th.id === themeId)
      || themes.find(th => th.id === DEFAULT_THEME_ID);
    if (meta) applyTheme(meta);
  }, [state.data.themes, state.data.themeId]);

  /* ---------- boot + intervals ---------- */
  useEffect(() => {
    refresh();
    // If the first load races the gateway (or a proxy hiccup), keep retrying —
    // otherwise the console would sit as an empty shell forever.
    const retry = setInterval(() => {
      if (!stateRef.current.data.status) refresh();
    }, 3000);
    const i1 = setInterval(refreshAkStatus, 1000);
    const i2 = setInterval(refreshStats, 1000);
    const i3 = setInterval(tick, 1000);
    return () => { clearInterval(retry); clearInterval(i1); clearInterval(i2); clearInterval(i3); };
  }, [refresh, refreshAkStatus, refreshStats, tick]);

  /* ---------- persist UI state + Android orientation ---------- */
  useEffect(() => { saveUiState(state.ui); }, [state.ui]);
  useEffect(() => {
    try {
      const w = window as unknown as { SakuraAndroid?: { setLandscape(b: boolean): void } };
      if (!w.SakuraAndroid) return;
      const u = state.ui;
      const wide = (u.tab === 'providers' && !!u.addState)
        || (u.tab === 'models' && (u.editingModel != null || !!u.groupEdit));
      w.SakuraAndroid.setLandscape(wide);
    } catch (_) { /* ignore */ }
  }, [state.ui.tab, state.ui.addState, state.ui.editingModel, state.ui.groupEdit]);

  const controller: Controller = {
    state, derived, dispatch, ui, setData, setTab, setLocalProv, setModelTab, setLang,
    pname, refresh, refreshAkStatus, runDryRun, putStrategy, findRealKey, refreshModelGroups,
    setTheme,
  };
  return <Ctx.Provider value={controller}>{children}</Ctx.Provider>;
}

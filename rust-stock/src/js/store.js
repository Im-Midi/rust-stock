// store.js — 全局状态 + 持久化（SQLite 权威，localStorage 缓存/浏览器回退）
import { inTauri, invoke } from './bridge.js';

export const DEFAULT_SETTINGS = { source: 'sina', interval: 10, key: '', aiBase: '', aiModel: '', closeAction: '' };

export const state = {
  settings: { ...DEFAULT_SETTINGS },
  watchlist: [],
  aiCache: {},
  recHistory: {}, // { "2026-06-06": [{code,name,score,reason}] }
  research: { groups: [], saved: [] }, // 深度调研收藏：3 个分组 + 整段文字
};

export const defaultGroups = () => ([
  { id: 'g1', name: '分组 1' }, { id: 'g2', name: '分组 2' }, { id: 'g3', name: '分组 3' },
]);

export async function storeGet(key, fallback) {
  if (inTauri) {
    try {
      const v = await invoke('db_get', { key });
      if (v != null) return JSON.parse(v);
    } catch (e) { console.warn('db_get 失败，回退 localStorage:', e); }
  }
  try {
    const v = localStorage.getItem('rs_' + key);
    if (v != null) return JSON.parse(v);
  } catch {}
  return fallback;
}

export function storeSet(key, obj) {
  const json = JSON.stringify(obj);
  try { localStorage.setItem('rs_' + key, json); } catch {}
  if (inTauri) invoke('db_set', { key, value: json }).catch(e => console.warn('db_set 失败:', e));
}

export async function loadAll() {
  state.settings = { ...DEFAULT_SETTINGS, ...(await storeGet('settings', {})) };
  state.watchlist = await storeGet('watchlist', []);
  state.aiCache = await storeGet('ai_cache', {});
  state.recHistory = await storeGet('rec_history', {});
  state.research = await storeGet('research', null) || { groups: defaultGroups(), saved: [] };
  if (!Array.isArray(state.research.groups) || state.research.groups.length !== 3) state.research.groups = defaultGroups();
  if (!Array.isArray(state.research.saved)) state.research.saved = [];
  // 迁移回写：老版本数据只在 localStorage，统一补进 SQLite，
  // 让挂件等其他窗口（共享 SQLite）也能读到
  if (inTauri) {
    storeSet('settings', state.settings);
    storeSet('watchlist', state.watchlist);
    storeSet('ai_cache', state.aiCache);
    storeSet('rec_history', state.recHistory);
    storeSet('research', state.research);
  }
}

export function saveSettings(s) { state.settings = s; storeSet('settings', s); }
export function saveWatch() { storeSet('watchlist', state.watchlist); }
export function saveAiCache() { storeSet('ai_cache', state.aiCache); }
export function saveRecHistory() { storeSet('rec_history', state.recHistory); }
export function saveResearch() { storeSet('research', state.research); }
export const today = () => new Date().toISOString().slice(0, 10);
export const aiReady = () => inTauri && !!state.settings.key;

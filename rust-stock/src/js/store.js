// store.js — 全局状态 + 持久化（SQLite 权威，localStorage 缓存/浏览器回退）
import { inTauri, invoke } from './bridge.js';

export const DEFAULT_SETTINGS = { source: 'sina', interval: 10, key: '', aiBase: '', aiModel: '' };

export const state = {
  settings: { ...DEFAULT_SETTINGS },
  watchlist: [],
  aiCache: {},
};

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
}

export function saveSettings(s) { state.settings = s; storeSet('settings', s); }
export function saveWatch() { storeSet('watchlist', state.watchlist); }
export function saveAiCache() { storeSet('ai_cache', state.aiCache); }
export const today = () => new Date().toISOString().slice(0, 10);
export const aiReady = () => inTauri && !!state.settings.key;

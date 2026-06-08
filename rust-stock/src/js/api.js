// api.js — Tauri 命令封装。失败返回 null，由调用方决定 mock 回退。
// 代码用统一格式（sh600519 / int_dji），各数据源的私有格式转换在 Rust 端。
import { inTauri, invoke } from './bridge.js';
import { state } from './store.js';

export const INDEX_CODES = ['sh000001', 'sz399001', 'sh000300', 'int_dji', 'int_nasdaq'];

export function normalizeCode(raw) {
  raw = (raw || '').trim().toLowerCase();
  if (/^(sh|sz)\d{6}$/.test(raw)) return raw;
  if (/^\d{6}$/.test(raw)) return (/^[569]/.test(raw) ? 'sh' : 'sz') + raw;
  return null;
}

export async function fetchQuotes(codes) {
  if (!inTauri) return null;
  try {
    const quotes = await invoke('fetch_quotes', { source: state.settings.source, codes });
    return (Array.isArray(quotes) && quotes.length) ? quotes : null;
  } catch (e) { console.warn('行情抓取失败:', e); return null; }
}

export async function listSources() {
  if (!inTauri) return null;
  try { return await invoke('list_sources'); } catch { return null; }
}

export async function fetchNews() {
  if (!inTauri) return null;
  try {
    const items = await invoke('fetch_news');
    return (Array.isArray(items) && items.length) ? items : null;
  } catch (e) { console.warn('快讯抓取失败:', e); return null; }
}

export async function fetchKline(code, period, count) {
  if (!inTauri) return null;
  try {
    const c = await invoke('fetch_kline', { code, period, count });
    return (Array.isArray(c) && c.length) ? c : null;
  } catch (e) { console.warn('K线获取失败:', e); return null; }
}

export async function searchStocks(keyword) {
  if (!inTauri) return null;
  try {
    const hits = await invoke('search_stocks', { keyword });
    return Array.isArray(hits) ? hits : [];
  } catch (e) { console.warn('搜索失败:', e); return []; }
}

export async function fetchSectors() {
  if (!inTauri) return null;
  try {
    const s = await invoke('fetch_sectors');
    return (Array.isArray(s) && s.length) ? s : null;
  } catch (e) { console.warn('板块获取失败:', e); return null; }
}

export async function fetchFundFlow(code) {
  if (!inTauri) return null;
  try { return await invoke('fetch_fund_flow', { code }); }
  catch (e) { console.warn('资金流获取失败:', e); return null; }
}

export async function classifyNews(titles) {
  if (!inTauri) return null;
  try {
    const r = await invoke('classify_news', { ...aiArgs(), titles });
    return Array.isArray(r) ? r : null;
  } catch (e) { console.warn('消息面分类失败:', e); return null; }
}

export async function fetchStockNews(codes) {
  if (!inTauri) return null;
  try {
    const items = await invoke('fetch_stock_news', { codes });
    return Array.isArray(items) ? items : null;
  } catch (e) { console.warn('自选股快讯失败:', e); return null; }
}

export async function fetchSentiment() {
  if (!inTauri) return null;
  try { return await invoke('fetch_sentiment'); }
  catch (e) { console.warn('情绪计算失败:', e); return null; }
}

// AI 命令统一带上 provider 配置（空串 = Rust 端用 DeepSeek 默认值）
function aiArgs() {
  return { key: state.settings.key, baseUrl: state.settings.aiBase, model: state.settings.aiModel };
}
export const analyzeStock = (name, code, price, changePct) =>
  invoke('analyze_stock', { ...aiArgs(), name, code, price, changePct });
export const explainSentiment = (score, label, detail) =>
  invoke('explain_sentiment', { ...aiArgs(), score, label, detail });
export const askAi = (question, history, mode) =>
  invoke('ask_ai', { ...aiArgs(), question, history, mode: mode || null });
export const aiRecommend = (context) =>
  invoke('ai_recommend', { ...aiArgs(), context });

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

// 历史主力资金流（近 N 个交易日，升序旧→新；失败 null 由调用方隐藏区块）
export async function fetchFlowHistory(code, days = 5) {
  if (!inTauri) return null;
  try {
    const r = await invoke('fetch_flow_history', { code, days });
    return (Array.isArray(r) && r.length) ? r : null;
  } catch (e) { console.warn('历史资金流失败:', e); return null; }
}

// 个股所属板块/概念（行业排最前；失败 null）
export async function fetchStockBoards(code) {
  if (!inTauri) return null;
  try {
    const r = await invoke('fetch_stock_boards', { code });
    return (Array.isArray(r) && r.length) ? r : null;
  } catch (e) { console.warn('所属板块失败:', e); return null; }
}

// 北向资金（沪深港通）近 5 个交易日成交金额（亿元，最新在前）。
// 净买额 2024-08 起已停披，接口只给成交额——展示侧如实标注。
export async function fetchNorthFlow() {
  if (!inTauri) return null;
  try {
    const r = await invoke('fetch_north_flow');
    return (Array.isArray(r) && r.length) ? r : null;
  } catch (e) { console.warn('北向资金失败:', e); return null; }
}

// 同花顺 A 股人气榜（小时榜 Top15，rank/code/name/涨跌幅/人气值/标签；失败 null）
export async function fetchHotStocks() {
  if (!inTauri) return null;
  try {
    const r = await invoke('fetch_hot_stocks');
    return (Array.isArray(r) && r.length) ? r : null;
  } catch (e) { console.warn('人气榜失败:', e); return null; }
}

// 个股分红历史（最新方案在前）。[] = 确实从未分红（合法结果）；null = 拉取失败
export async function fetchDividends(code) {
  if (!inTauri) return null;
  try {
    const r = await invoke('fetch_dividends', { code });
    return Array.isArray(r) ? r : null;
  } catch (e) { console.warn('分红获取失败:', e); return null; }
}

// 个股股本/市值快照（总股本/流通股/总市值/流通市值 + 动态PE/PB；失败 null）
export async function fetchShareInfo(code) {
  if (!inTauri) return null;
  try { return await invoke('fetch_share_info', { code }); }
  catch (e) { console.warn('股本市值失败:', e); return null; }
}

// 核心财务指标（最新报告期：营收/净利+同比、EPS、ROE、毛利率、负债率；
// 未披露字段为 null（前端显示 "--"），整体失败 null → 隐藏区块）
export async function fetchFinancials(code) {
  if (!inTauri) return null;
  try { return await invoke('fetch_financials', { code }); }
  catch (e) { console.warn('财务指标失败:', e); return null; }
}

// 融资融券个股余额（最近 2 个交易日，最新在前）。[] = 非两融标的（合法）；null = 失败
export async function fetchMargin(code) {
  if (!inTauri) return null;
  try {
    const r = await invoke('fetch_margin', { code });
    return Array.isArray(r) ? r : null;
  } catch (e) { console.warn('两融获取失败:', e); return null; }
}

// 个股龙虎榜明细（最近 5 次上榜，最新在前）。[] = 从未上榜（合法）；null = 失败
export async function fetchLhbDetail(code) {
  if (!inTauri) return null;
  try {
    const r = await invoke('fetch_lhb_detail', { code });
    return Array.isArray(r) ? r : null;
  } catch (e) { console.warn('龙虎榜获取失败:', e); return null; }
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
export const analyzeStock = (name, code, price, changePct, context, mode) =>
  invoke('analyze_stock', { ...aiArgs(), name, code, price, changePct, context: context || null, mode: mode || null });
export const explainSentiment = (score, label, detail) =>
  invoke('explain_sentiment', { ...aiArgs(), score, label, detail });
export const askAi = (question, history, mode) =>
  invoke('ask_ai', { ...aiArgs(), question, history, mode: mode || null });
export const aiRecommend = (context) =>
  invoke('ai_recommend', { ...aiArgs(), context });

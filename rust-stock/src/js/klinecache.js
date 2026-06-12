// klinecache.js — 共享K线缓存层（推荐缩略图 / K线页 / 战绩回算共用一份数据）
// 为什么需要它（手机端尤其）：
//   · 同一支股票的日K此前会被推荐缩略图(30根)、K线页(250根)、战绩回算(60根)各拉一遍，
//     安卓 rustls 对东财的偶发掐线让每次请求都是一次"赌"，请求越多越容易出现白图。
//   · 缓存按 code|period 存"整段"，不同长度共享同一份（取尾部切片），命中即 0 请求。
// 能力：
//   · TTL 命中（默认 5 分钟）：推荐页/K线页来回切换不再重复打接口
//   · in-flight 去重：同一 code|period|count 并发只发一次请求
//   · last-good：拉取失败但有历史成功数据 → 返回旧数据并标 stale=true，
//     瞬时网络波动不再把已经画出来的图打成白板（调用方可标注"缓存数据"）
//   · mapLimit：有界并发工具（推荐页缩略图并发 3 支，不再串行排队几分钟）
import { fetchKline } from './api.js';
import { storeGet, storeSet } from './store.js';

const TTL = 5 * 60 * 1000;
const cache = new Map();    // `${code}|${period}` -> { candles, count, ts }
const inflight = new Map(); // `${code}|${period}|${count}` -> Promise

// ---------- 持久化（SQLite，经 storeSet/storeGet，与 watch_quotes_cache 同机制）----------
// 目标：冷启动（哪怕离线）也能立即画出上次成功的真实K线/缩略图。
// 约束：只存日K（周/月可重拉、占比小收益低）；最多 40 支（按最近成功时间挑）；
//      每支只留最近 120 根（缩略图 30 根/筹码 80 根窗口都够用，K线页要 250 根
//      会照常发起网络请求，离线时由 last-good 返回这 120 根真实数据）。
// 写入防抖 3s：一轮预热会连续成功多支，合并成一次 SQLite 写。
const PERSIST_KEY = 'kline_cache';
const PERSIST_MAX_CODES = 40;
const PERSIST_MAX_CANDLES = 120;
let persistTimer = null;

function schedulePersist() {
  clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    try {
      const entries = [...cache.entries()]
        .filter(([k, v]) => k.endsWith('|day') && v && v.candles && v.candles.length)
        .sort((a, b) => b[1].ts - a[1].ts)
        .slice(0, PERSIST_MAX_CODES);
      const obj = {};
      for (const [k, v] of entries) {
        const candles = v.candles.slice(-PERSIST_MAX_CANDLES);
        obj[k] = { candles, count: Math.min(v.count, candles.length), ts: v.ts };
      }
      storeSet(PERSIST_KEY, obj);
    } catch (e) { console.warn('K线缓存持久化失败:', e); }
  }, 3000);
}

// 启动时回填内存缓存（main.js 在首屏渲染前调用一次）。
// 回填的 ts 是上次成功时间：TTL 已过会照常重新拉取；拉取失败（离线/掐线）
// 则由 last-good 机制返回这批真实历史数据 → 冷启动绝不白屏，也绝不造假数据。
export async function hydrateKlineCache() {
  try {
    const saved = await storeGet(PERSIST_KEY, null);
    if (!saved || typeof saved !== 'object') return;
    for (const [k, v] of Object.entries(saved)) {
      if (cache.has(k) || !v || !Array.isArray(v.candles) || !v.candles.length) continue;
      cache.set(k, { candles: v.candles, count: v.count || v.candles.length, ts: v.ts || 0 });
    }
  } catch (e) { console.warn('K线缓存回填失败:', e); }
}

const tail = (arr, n) => (arr.length > n ? arr.slice(-n) : arr);

// 取K线（带缓存）。返回 { candles, stale, ts }；彻底失败且无任何历史数据 → null。
// stale=true 表示本次拉取失败、candles 是上一次成功的旧数据（last-good）。
export async function getKline(code, period, count) {
  const key = `${code}|${period}`;
  const hit = cache.get(key);
  if (hit && hit.count >= count && Date.now() - hit.ts < TTL) {
    return { candles: tail(hit.candles, count), stale: false, ts: hit.ts };
  }
  const fkey = `${key}|${count}`;
  if (inflight.has(fkey)) return inflight.get(fkey);
  const p = (async () => {
    try {
      const k = await fetchKline(code, period, count);
      if (k && k.length) {
        const prev = cache.get(key);
        let merged = k;
        // 新拉的段比缓存短（如刚拉30根、缓存里有250根）→ 按日期拼接，保留更长历史
        if (prev && prev.candles.length > k.length) {
          const first = k[0].date;
          merged = prev.candles.filter(c => c.date < first).concat(k);
        }
        const cnt = Math.max(count, prev ? prev.count : 0);
        const ts = Date.now();
        cache.set(key, { candles: merged, count: cnt, ts });
        if (period === 'day') schedulePersist(); // 成功数据落 SQLite（防抖合并）
        return { candles: tail(merged, count), stale: false, ts };
      }
    } catch (e) { console.warn('K线缓存层拉取异常:', e); }
    // 失败 → last-good：有旧数据就用旧数据（无论 TTL），绝不让已显示过的图变白
    const old = cache.get(key);
    if (old && old.candles.length) {
      return { candles: tail(old.candles, count), stale: true, ts: old.ts };
    }
    return null;
  })().finally(() => inflight.delete(fkey));
  inflight.set(fkey, p);
  return p;
}

// 有界并发 map：最多 limit 个并发执行 fn(item, index)，结果按原顺序返回（失败位为 null）
export async function mapLimit(items, limit, fn) {
  const out = new Array(items.length);
  let i = 0;
  const workers = Array.from({ length: Math.max(1, Math.min(limit, items.length)) }, async () => {
    while (i < items.length) {
      const idx = i++;
      try { out[idx] = await fn(items[idx], idx); } catch { out[idx] = null; }
    }
  });
  await Promise.all(workers);
  return out;
}

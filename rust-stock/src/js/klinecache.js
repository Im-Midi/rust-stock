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

const TTL = 5 * 60 * 1000;
const cache = new Map();    // `${code}|${period}` -> { candles, count, ts }
const inflight = new Map(); // `${code}|${period}|${count}` -> Promise

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

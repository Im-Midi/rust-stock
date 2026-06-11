// chip.js — 筹码分布（成交量在价位上的近似分布）
// 算法移植自 ArvinLovegood/go-stock 的 backend/data/chip_distribution.go（Apache-2.0，见 NOTICE）。
// 思想：历史筹码按每日换手率衰减（保留比例 = 1 - 换手率），当日成交量以「成本中枢」
//   （优先日 VWAP=成交额/成交量，否则典型价 (高+低+收)/3）为中心，做高斯核分摊到 [低,高] 与各价格 bin 的交集。
// 输入：日K数组（需 high/low/close/volume，最好含 turnover% 与 amount 元）。
// 输出：{ current, avgCost, profitRatio, minPrice, maxPrice, sumVol, items:[{price,vol,ratio}] }

const isFin = (x) => Number.isFinite(x);
const clamp = (x, lo, hi) => Math.min(hi, Math.max(lo, x));

// 换手率 → 0~1（兼容数字 2.34 / 字符串 "2.34%"）
function pct(t) {
  if (t == null) return 0;
  const v = typeof t === 'number' ? t : parseFloat(String(t).replace('%', '').trim());
  return isFin(v) ? v / 100 : 0;
}

// 成本中枢：优先 VWAP（成交额/成交量，自动校正“手”单位），否则典型价/中值
function costCenter(low, high, open, close, vol, amount) {
  if (!(low > 0) || !(high > 0) || high < low) return isFin(low) && isFin(high) ? (low + high) / 2 : 0;
  if (amount > 0 && vol > 0) {
    let vwap = amount / vol;
    if (vwap > high * 1.5) vwap /= 100; // 成交量以“手”(100股)计时，amount/vol 会放大 100 倍，校正回价格量级
    if (isFin(vwap) && vwap > 0) return clamp(vwap, low, high);
  }
  if (close > 0 && isFin(close)) return clamp((high + low + close) / 3, low, high);
  if (open > 0 && close > 0) return clamp((high + low + open + close) / 4, low, high);
  return (high + low) / 2;
}

// 高斯核：把当日 vol 分摊到 [low,high] 覆盖的 bin（一字板单档；权重为 0 回退均匀）
function addKernel(dist, bins, minP, width, low, high, vol, center) {
  if (vol <= 0 || low <= 0 || high <= 0) return;
  if (high < low) { const t = low; low = high; high = t; }
  const span = high - low;
  let loIdx = Math.floor((low - minP) / width);
  let hiIdx = Math.floor((high - minP) / width);
  loIdx = Math.max(0, Math.min(bins - 1, loIdx));
  hiIdx = Math.max(0, Math.min(bins - 1, hiIdx));
  if (hiIdx < loIdx) return;
  if (span < 1e-9 * Math.max(1, high)) { // 一字板
    let i = Math.floor(((low + high) / 2 - minP) / width);
    i = Math.max(0, Math.min(bins - 1, i));
    dist[i] += vol; return;
  }
  let m = isFin(center) ? center : (low + high) / 2;
  m = clamp(m, low, high);
  const sigma = Math.max(span * 0.18, Math.max(high * 1e-6, 1e-6));
  let wsum = 0;
  for (let i = loIdx; i <= hiIdx; i++) {
    const bc = minP + (i + 0.5) * width;
    if (bc < low || bc > high) continue;
    const d = (bc - m) / sigma; wsum += Math.exp(-0.5 * d * d);
  }
  if (wsum <= 0) {
    const add = vol / (hiIdx - loIdx + 1);
    for (let i = loIdx; i <= hiIdx; i++) dist[i] += add; return;
  }
  for (let i = loIdx; i <= hiIdx; i++) {
    const bc = minP + (i + 0.5) * width;
    if (bc < low || bc > high) continue;
    const d = (bc - m) / sigma;
    dist[i] += vol * Math.exp(-0.5 * d * d) / wsum;
  }
}

export function calcChips(candles, bins = 80) {
  if (!candles || !candles.length) return null;
  bins = Math.max(1, Math.min(300, bins | 0 || 80));
  let minP = Infinity, maxP = 0;
  for (const k of candles) {
    const lo = +k.low || 0, hi = +k.high || 0;
    if (lo > 0 && lo < minP) minP = lo;
    if (hi > 0 && hi > maxP) maxP = hi;
  }
  if (!(minP > 0) || !(maxP > 0) || maxP < minP) return null;
  if (maxP === minP) maxP = minP * 1.001;
  const width = (maxP - minP) / bins;
  if (!(width > 0)) return null;

  const dist = new Array(bins).fill(0);
  for (const k of candles) {
    let turn = pct(k.turnover);
    if (turn < 0) turn = 0; if (turn > 0.98) turn = 0.98;
    const remain = 1 - turn;
    for (let i = 0; i < bins; i++) dist[i] *= remain;
    let low = +k.low || 0, high = +k.high || 0;
    const vol = +k.volume || 0;
    if (vol <= 0 || low <= 0 || high <= 0) continue;
    if (high < low) { const t = low; low = high; high = t; }
    const center = costCenter(low, high, +k.open || 0, +k.close || 0, vol, +k.amount || 0);
    addKernel(dist, bins, minP, width, low, high, vol, center);
  }

  let sum = 0; for (const v of dist) sum += v;
  let cur = +candles[candles.length - 1].close || 0;
  if (cur <= 0) cur = +candles[candles.length - 1].high || 0;
  const items = []; let avgCost = 0, profitVol = 0;
  for (let i = 0; i < bins; i++) {
    const center = minP + (i + 0.5) * width;
    const vol = dist[i];
    items.push({ price: center, vol, ratio: sum > 0 ? vol / sum : 0 });
    avgCost += vol * center;
    if (center <= cur) profitVol += vol;
  }
  if (sum > 0) avgCost /= sum;
  return {
    current: cur, avgCost, profitRatio: sum > 0 ? profitVol / sum : 0,
    minPrice: minP, maxPrice: maxP, sumVol: sum, items,
  };
}

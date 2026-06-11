// mytt.js — 通达信口径技术指标（移植自 mpquant/MyTT，MIT，见 NOTICE）
// 关键：中国式 SMA(S,N,M)=Y=(M*X+(N-M)*Y')/N（ewm alpha=M/N），需 ≥120 周期收敛——
// 本项目 K线默认取 250 根，足够与通达信/同花顺/雪球对齐到小数点后 2 位。

export function EMA(S, N) {
  const out = new Array(S.length).fill(NaN);
  const k = 2 / (N + 1); let prev;
  for (let i = 0; i < S.length; i++) {
    const x = S[i];
    if (!Number.isFinite(x)) { out[i] = prev ?? NaN; continue; }
    prev = (prev === undefined) ? x : x * k + prev * (1 - k);
    out[i] = prev;
  }
  return out;
}
// 中国式 SMA
export function SMA(S, N, M = 1) {
  const out = new Array(S.length).fill(NaN); let prev;
  for (let i = 0; i < S.length; i++) {
    const x = S[i];
    if (!Number.isFinite(x)) { out[i] = prev ?? NaN; continue; }
    prev = (prev === undefined) ? x : (M * x + (N - M) * prev) / N;
    out[i] = prev;
  }
  return out;
}
export function MA(S, N) {
  const out = new Array(S.length).fill(NaN); let sum = 0;
  for (let i = 0; i < S.length; i++) {
    sum += S[i];
    if (i >= N) sum -= S[i - N];
    if (i >= N - 1) out[i] = sum / N;
  }
  return out;
}
export function HHV(S, N) { const o = new Array(S.length).fill(NaN); for (let i = 0; i < S.length; i++) { let m = -Infinity; for (let j = Math.max(0, i - N + 1); j <= i; j++) m = Math.max(m, S[j]); o[i] = m; } return o; }
export function LLV(S, N) { const o = new Array(S.length).fill(NaN); for (let i = 0; i < S.length; i++) { let m = Infinity; for (let j = Math.max(0, i - N + 1); j <= i; j++) m = Math.min(m, S[j]); o[i] = m; } return o; }

export function MACD(close, S = 12, L = 26, M = 9) {
  const es = EMA(close, S), el = EMA(close, L);
  const dif = es.map((v, i) => v - el[i]);
  const dea = EMA(dif, M);
  const macd = dif.map((v, i) => (v - dea[i]) * 2);
  return { dif, dea, macd };
}
export function KDJ(high, low, close, N = 9, M1 = 3, M2 = 3) {
  const llv = LLV(low, N), hhv = HHV(high, N);
  const rsv = close.map((c, i) => { const d = hhv[i] - llv[i]; return d ? (c - llv[i]) / d * 100 : 0; });
  const k = SMA(rsv, M1, 1), d = SMA(k, M2, 1), j = k.map((v, i) => 3 * v - 2 * d[i]);
  return { k, d, j };
}
export function RSI(close, N = 6) {
  const dif = close.map((c, i) => (i ? c - close[i - 1] : 0));
  const su = SMA(dif.map(x => Math.max(x, 0)), N, 1);
  const sa = SMA(dif.map(x => Math.abs(x)), N, 1);
  return su.map((v, i) => (sa[i] ? v / sa[i] * 100 : 0));
}
export function BOLL(close, N = 20, P = 2) {
  const mid = MA(close, N);
  const std = close.map((_, i) => {
    if (i < N - 1) return NaN;
    const m = mid[i]; let s = 0;
    for (let j = i - N + 1; j <= i; j++) s += (close[j] - m) ** 2;
    return Math.sqrt(s / N);
  });
  return { up: mid.map((m, i) => m + P * std[i]), mid, low: mid.map((m, i) => m - P * std[i]) };
}

// 现值 + 金叉/死叉摘要（喂给 UI 与（将来）AI 注入层）
export function indicatorSummary(candles) {
  if (!candles || candles.length < 35) return null;
  const close = candles.map(c => +c.close), high = candles.map(c => +c.high), low = candles.map(c => +c.low);
  const { dif, dea, macd } = MACD(close);
  const { k, d, j } = KDJ(high, low, close);
  const r6 = RSI(close, 6), r12 = RSI(close, 12);
  const { up, mid, low: bl } = BOLL(close);
  const i = close.length - 1, p = i - 1;
  const cross = (a, b) => (a[p] <= b[p] && a[i] > b[i]) ? 'gold' : (a[p] >= b[p] && a[i] < b[i]) ? 'dead' : '';
  return {
    macd: { dif: dif[i], dea: dea[i], macd: macd[i], cross: cross(dif, dea) },
    kdj: { k: k[i], d: d[i], j: j[i], cross: cross(k, d) },
    rsi: { r6: r6[i], r12: r12[i] },
    boll: { up: up[i], mid: mid[i], low: bl[i] },
  };
}

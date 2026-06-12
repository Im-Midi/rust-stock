// pages/kline.js — K线图（canvas 蜡烛图 + MA5/MA10 + 成交量）
// 交互：滚轮缩放（光标锚定）、按住拖动平移、日/周/月切换。自选股点名称进入。
import { fetchQuotes, fetchFundFlow, analyzeStock } from '../api.js';
import { getKline } from '../klinecache.js';
import { calcChips } from '../chip.js';
import { indicatorSummary } from '../mytt.js';
import { switchPage } from '../router.js';
import { showAnalysis } from './analysis.js';
import { flashHint } from '../ui.js';
import { aiReady } from '../store.js';
import { inTauri, isMobile } from '../bridge.js';

const cur = { code: null, name: '', period: 'day' };
let data = [];                      // 全量K线缓存（最多 250 根）
let view = { start: 0, count: 90 }; // 当前可视窗口
const FETCH_N = 250;

// 浏览器预览：按代码种子生成稳定的随机游走K线
function mockCandles(code, n = FETCH_N) {
  let seed = [...code].reduce((a, c) => a + c.charCodeAt(0), 7);
  const rand = () => { seed = (seed * 9301 + 49297) % 233280; return seed / 233280; };
  let price = 20 + (seed % 180);
  const out = [];
  const d = new Date();
  d.setDate(d.getDate() - n);
  for (let i = 0; i < n; i++) {
    d.setDate(d.getDate() + 1);
    const open = price;
    const drift = (rand() - 0.48) * price * 0.04;
    const close = Math.max(1, open + drift);
    const high = Math.max(open, close) * (1 + rand() * 0.015);
    const low = Math.min(open, close) * (1 - rand() * 0.015);
    const vol = 10000 + rand() * 90000;
    out.push({
      date: d.toISOString().slice(0, 10),
      open, close, high, low,
      volume: vol,
      amount: vol * 100 * (high + low + close) / 3,
      turnover: 1 + rand() * 4,
    });
    price = close;
  }
  return out;
}

// 基于全量数据算 MA，再切片，保证窗口左缘的均线也准确
function maSlice(n, start, count) {
  const out = [];
  for (let i = start; i < start + count; i++) {
    if (i < n - 1) { out.push(null); continue; }
    let s = 0;
    for (let j = i - n + 1; j <= i; j++) s += data[j].close;
    out.push(s / n);
  }
  return out;
}

function draw() {
  const candles = data.slice(view.start, view.start + view.count);
  if (!candles.length) return;
  const cv = document.getElementById('klineCanvas');
  const ctx = cv.getContext('2d');
  const W = cv.width, H = cv.height;
  ctx.clearRect(0, 0, W, H);

  const padL = 8, padR = 64, padT = 14;
  const priceH = H * 0.68;
  const volTop = priceH + 24;
  const volH = H - volTop - 10;
  const plotW = W - padL - padR;

  const hi = Math.max(...candles.map(c => c.high));
  const lo = Math.min(...candles.map(c => c.low));
  const span = hi - lo || 1;
  const y = p => padT + (hi - p) / span * (priceH - padT);
  const maxVol = Math.max(...candles.map(c => c.volume)) || 1;

  const n = candles.length;
  const step = plotW / n;
  const bw = Math.max(1.5, step * 0.62);

  const cs = getComputedStyle(document.body);
  const UP = cs.getPropertyValue('--up').trim() || '#ff4d4f';
  const DOWN = cs.getPropertyValue('--down').trim() || '#14c87d';
  const GRID = cs.getPropertyValue('--line-soft').trim() || 'rgba(255,255,255,.05)';
  const AXIS = cs.getPropertyValue('--txt-3').trim() || '#5c6470';
  const HOLLOW = cs.getPropertyValue('--surface').trim() || '#12151c';

  ctx.font = '16px "DM Mono", monospace';
  ctx.textBaseline = 'middle';
  for (let i = 0; i <= 4; i++) {
    const p = hi - span * i / 4;
    const yy = y(p);
    ctx.strokeStyle = GRID;
    ctx.beginPath(); ctx.moveTo(padL, yy); ctx.lineTo(W - padR, yy); ctx.stroke();
    ctx.fillStyle = AXIS;
    ctx.fillText(p.toFixed(2), W - padR + 6, yy);
  }

  candles.forEach((c, i) => {
    const x = padL + i * step + step / 2;
    const up = c.close >= c.open;
    ctx.strokeStyle = ctx.fillStyle = up ? UP : DOWN;
    ctx.beginPath(); ctx.moveTo(x, y(c.high)); ctx.lineTo(x, y(c.low)); ctx.stroke();
    const yo = y(c.open), yc = y(c.close);
    const top = Math.min(yo, yc), hgt = Math.max(1.5, Math.abs(yo - yc));
    if (up) { ctx.fillStyle = HOLLOW; ctx.fillRect(x - bw / 2, top, bw, hgt); ctx.strokeRect(x - bw / 2, top, bw, hgt); }
    else ctx.fillRect(x - bw / 2, top, bw, hgt);
    const vh = c.volume / maxVol * volH;
    ctx.fillStyle = up ? 'rgba(255,77,79,.5)' : 'rgba(20,200,125,.5)';
    ctx.fillRect(x - bw / 2, H - 10 - vh, bw, vh);
  });

  const drawMa = (maData, color) => {
    ctx.strokeStyle = color; ctx.lineWidth = 1.6;
    ctx.beginPath();
    let started = false;
    maData.forEach((v, i) => {
      if (v == null) return;
      const x = padL + i * step + step / 2;
      if (!started) { ctx.moveTo(x, y(v)); started = true; }
      else ctx.lineTo(x, y(v));
    });
    ctx.stroke(); ctx.lineWidth = 1;
  };
  drawMa(maSlice(5, view.start, view.count), '#f5a623');
  drawMa(maSlice(10, view.start, view.count), '#4d8dff');

  ctx.fillStyle = AXIS;
  ctx.textBaseline = 'alphabetic';
  ctx.fillText(candles[0].date, padL, priceH + 18);
  const lastD = candles[n - 1].date;
  ctx.fillText(lastD, W - padR - ctx.measureText(lastD).width, priceH + 18);

  const last = data[data.length - 1]; // 信息栏永远显示最新一根
  const chg = (last.close - last.open) / last.open * 100;
  document.getElementById('klineInfo').innerHTML = `
    <span>开 <b>${last.open.toFixed(2)}</b></span>
    <span>收 <b class="${last.close >= last.open ? 'up-c' : 'down-c'}">${last.close.toFixed(2)}</b></span>
    <span>高 <b>${last.high.toFixed(2)}</b></span>
    <span>低 <b>${last.low.toFixed(2)}</b></span>
    <span class="${chg >= 0 ? 'up-c' : 'down-c'}">${chg >= 0 ? '+' : ''}${chg.toFixed(2)}%</span>
    <span style="color:#f5a623">— MA5</span>
    <span style="color:#4d8dff">— MA10</span>`;
}

function resetView() {
  view.count = Math.min(90, data.length);
  view.start = data.length - view.count;
}

let loadSeq = 0; // 防过期响应：快速切股/切周期时，旧请求回来不覆盖新图
async function load() {
  const seq = ++loadSeq;
  document.getElementById('klineMeta').textContent = '加载中…';
  let candles = null, stale = false, staleTs = 0;
  if (inTauri) {
    // 共享K线缓存：TTL 命中 0 请求；拉取失败但有上次成功数据 → last-good（stale）
    const res = await getKline(cur.code, cur.period, FETCH_N);
    if (seq !== loadSeq) return; // 已经切到别的股票/周期，丢弃过期结果
    if (res && res.candles.length) { candles = res.candles; stale = res.stale; staleTs = res.ts; }
  }
  let mocked = false;
  if (!candles) {
    if (inTauri) {
      // 真机：K线拉取失败且无任何历史缓存，绝不用模拟数据冒充
      //（否则会把如 2.59 显示成假的 200 多，并误导筹码/指标）
      data = [];
      const cv = document.getElementById('klineCanvas');
      if (cv) cv.getContext('2d').clearRect(0, 0, cv.width, cv.height);
      const info = document.getElementById('klineInfo'); if (info) info.innerHTML = '';
      const panel = document.getElementById('chipPanel'); if (panel) panel.style.display = 'none';
      const indi = document.getElementById('klineIndi'); if (indi) indi.innerHTML = '';
      document.getElementById('klineMeta').textContent =
        `${cur.code.toUpperCase()} · K线加载失败（网络波动）· 点周期键或重进重试（不会用模拟数据冒充真实价）`;
      return;
    }
    candles = mockCandles(cur.code); mocked = true; // 仅浏览器预览用假数据
  }
  data = candles;
  resetView();
  draw();
  const gesture = isMobile ? '双指缩放 / 单指平移' : '滚轮缩放 / 拖动平移';
  let srcNote = mocked ? ' · 预览模拟数据' : '';
  if (stale) {
    const t = new Date(staleTs);
    const hm = `${String(t.getHours()).padStart(2, '0')}:${String(t.getMinutes()).padStart(2, '0')}`;
    srcNote = ` · ⚠ 网络波动，显示 ${hm} 缓存数据（切周期可重试）`;
  }
  document.getElementById('klineMeta').textContent =
    `${cur.code.toUpperCase()} · 共 ${data.length} 根 · 前复权 · ${gesture}` + srcNote;
  drawChip();
  drawIndicators();
}

// MyTT 通达信口径指标现值（仅日K口径）
function drawIndicators() {
  const el = document.getElementById('klineIndi');
  if (!el) return;
  const s = (cur.period === 'day') ? indicatorSummary(data) : null;
  if (!s) { el.innerHTML = ''; return; }
  const cx = c => c === 'gold' ? '<span class="gold">金叉</span>' : c === 'dead' ? '<span class="dead">死叉</span>' : '';
  const f = (v, n = 2) => Number.isFinite(v) ? v.toFixed(n) : '--';
  el.innerHTML =
    `<span>MACD ${cx(s.macd.cross)} DIF <b>${f(s.macd.dif)}</b> DEA <b>${f(s.macd.dea)}</b> M <b>${f(s.macd.macd)}</b></span>` +
    `<span>KDJ ${cx(s.kdj.cross)} K <b>${f(s.kdj.k, 1)}</b> D <b>${f(s.kdj.d, 1)}</b> J <b>${f(s.kdj.j, 1)}</b></span>` +
    `<span>RSI6 <b>${f(s.rsi.r6, 1)}</b> RSI12 <b>${f(s.rsi.r12, 1)}</b></span>` +
    `<span>BOLL 上<b>${f(s.boll.up)}</b> 中<b>${f(s.boll.mid)}</b> 下<b>${f(s.boll.low)}</b></span>`;
}

// AI 解读：把已算好的筹码/指标/行情作为真实数据注入 analyze_stock（数据注入层）
let aiBusy = false;
async function aiAnalyze() {
  if (!aiReady()) { flashHint('先在设置页接入 AI API Key'); return; }
  if (!data.length) { flashHint('K线未就绪'); return; }
  if (aiBusy) { flashHint('AI 正在解读，稍候'); return; }
  const last = data[data.length - 1];
  const chg = last.open ? (last.close - last.open) / last.open * 100 : 0;
  const parts = [];
  parts.push(`${cur.name}（${cur.code.toUpperCase()}）现价 ${last.close.toFixed(2)}，今日 ${chg >= 0 ? '+' : ''}${chg.toFixed(2)}%`);
  const chip = calcChips(data, 80);
  if (chip) parts.push(`筹码分布：获利比例 ${(chip.profitRatio * 100).toFixed(1)}%，平均成本 ${chip.avgCost.toFixed(2)}，成本区间 ${chip.minPrice.toFixed(2)}–${chip.maxPrice.toFixed(2)}`);
  const ind = indicatorSummary(data);
  if (ind) {
    const cz = c => c === 'gold' ? '金叉' : c === 'dead' ? '死叉' : '无交叉';
    parts.push(`技术指标(通达信口径)：MACD ${cz(ind.macd.cross)}(DIF ${ind.macd.dif.toFixed(2)}/DEA ${ind.macd.dea.toFixed(2)})；KDJ ${cz(ind.kdj.cross)}(K${ind.kdj.k.toFixed(0)}/D${ind.kdj.d.toFixed(0)}/J${ind.kdj.j.toFixed(0)})；RSI6 ${ind.rsi.r6.toFixed(0)}；BOLL 上${ind.boll.up.toFixed(2)}/中${ind.boll.mid.toFixed(2)}/下${ind.boll.low.toFixed(2)}`);
  }
  if (data.length >= 20) { const a = data[data.length - 20].close; if (a) parts.push(`近20交易日累计 ${((last.close - a) / a * 100).toFixed(1)}%`); }
  const context = '截至最新交易日，本股真实数据如下：\n' + parts.join('\n');
  aiBusy = true;
  flashHint('AI 解读中…（结合筹码/指标）');
  try {
    const res = await analyzeStock(cur.name, cur.code, last.close, chg, context, 'technical');
    if (res && typeof res.score === 'number') {
      showAnalysis({
        title: `${cur.name} · 盘面技术解读`,
        score: res.score,
        text: res.analysis,
        meta: `${cur.code.toUpperCase()} · 已注入筹码/指标/行情实时数据 · 仅供参考，不构成投资建议`,
        back: 'kline',
      });
    } else { flashHint('AI 未返回有效结果'); }
  } catch (e) { flashHint('AI 解读失败：' + e); }
  finally { aiBusy = false; }
}

// 筹码分布：用全量日K（含换手率/成交额）算分布并绘制
function drawChip() {
  const panel = document.getElementById('chipPanel');
  const cv = document.getElementById('chipCanvas');
  if (!panel || !cv) return;
  const res = (cur.period === 'day') ? calcChips(data, 80) : null; // 仅日K口径有意义
  if (!res || !res.sumVol) { panel.style.display = 'none'; return; }
  panel.style.display = 'block';
  const ctx = cv.getContext('2d');
  const W = cv.width, H = cv.height;
  ctx.clearRect(0, 0, W, H);
  const padT = 10, padB = 10, padR = 64, padL = 8;
  const plotH = H - padT - padB;
  const lo = res.minPrice, hi = res.maxPrice, span = (hi - lo) || 1;
  const y = p => padT + (hi - p) / span * plotH;
  const maxR = Math.max(...res.items.map(it => it.ratio)) || 1;
  const cs = getComputedStyle(document.body);
  const UP = cs.getPropertyValue('--up').trim() || '#ff4d4f';
  const barMaxW = W - padR - padL - 2;
  const binH = plotH / res.items.length;
  res.items.forEach(it => {
    const w = it.ratio / maxR * barMaxW;
    if (w <= 0) return;
    const yy = y(it.price);
    ctx.fillStyle = it.price <= res.current ? 'rgba(255,77,79,.55)' : 'rgba(20,200,125,.5)';
    ctx.fillRect(W - padR - w, yy - binH / 2, w, Math.max(1, binH * 0.9));
  });
  const line = (p, color, dash, label) => {
    if (!(p > 0)) return;
    const yy = y(p);
    ctx.save(); ctx.setLineDash(dash); ctx.strokeStyle = color; ctx.lineWidth = 1.2;
    ctx.beginPath(); ctx.moveTo(padL, yy); ctx.lineTo(W - padR, yy); ctx.stroke(); ctx.restore();
    ctx.fillStyle = color; ctx.font = '15px "DM Mono", monospace'; ctx.textBaseline = 'middle';
    ctx.fillText(label, W - padR + 5, yy);
  };
  line(res.current, UP, [], res.current.toFixed(2));
  line(res.avgCost, '#f5a623', [4, 3], res.avgCost.toFixed(2));
  const pr = res.profitRatio * 100;
  document.getElementById('chipStat').innerHTML =
    `获利 <b class="${pr >= 50 ? 'up-c' : 'down-c'}">${pr.toFixed(1)}%</b> · 均价 <b>${res.avgCost.toFixed(2)}</b> · 现价 <b>${res.current.toFixed(2)}</b>`;
}

// 金额格式化：元 → 亿/万
function fmtAmt(yuan) {
  const a = Math.abs(yuan);
  const sign = yuan >= 0 ? '+' : '-';
  if (a >= 1e8) return `${sign}${(a / 1e8).toFixed(2)}亿`;
  if (a >= 1e4) return `${sign}${(a / 1e4).toFixed(0)}万`;
  return `${sign}${a.toFixed(0)}`;
}

async function loadDetail(code) {
  const el = document.getElementById('stockDetail');
  el.innerHTML = '';
  // 并行取行情快照 + 资金流（换手率随资金流接口一起来）
  const [quotes, ff] = await Promise.all([fetchQuotes([code]), fetchFundFlow(code)]);
  const q = quotes && quotes[0];
  let html = '';
  if (q) {
    const volWan = (q.volume / 1e4).toFixed(0); // 手→万手 约略
    const amp = q.prev_close ? (q.high - q.low) / q.prev_close * 100 : 0;
    const turn = (ff && ff.turnover) ? ff.turnover.toFixed(2) + '%' : '--';
    const chgCls = q.change >= 0 ? 'up-c' : 'down-c';
    const cells = [
      ['今开', q.open ? q.open.toFixed(2) : '--', ''],
      ['昨收', q.prev_close ? q.prev_close.toFixed(2) : '--', ''],
      ['最高', q.high ? q.high.toFixed(2) : '--', 'up-c'],
      ['最低', q.low ? q.low.toFixed(2) : '--', 'down-c'],
      ['换手率', turn, ''],
      ['振幅', amp ? amp.toFixed(2) + '%' : '--', ''],
      ['成交量', q.volume ? volWan + '万手' : '--', ''],
      ['成交额', q.amount ? (q.amount / 1e8).toFixed(2) + '亿' : '--', ''],
      ['涨跌额', (q.change >= 0 ? '+' : '') + q.change.toFixed(2), chgCls],
    ];
    html += '<div class="sd-grid">' + cells.map(c =>
      `<div class="sd-cell"><div class="k">${c[0]}</div><div class="v ${c[2]}">${c[1]}</div></div>`).join('') + '</div>';
  }
  // 资金流（红=净流入，绿=净流出；每行带净占比）。腾讯兜底只有换手率、5档全 0 → 隐藏资金流条
  const hasFlow = ff && [ff.main, ff.super_big, ff.big, ff.mid, ff.small].some(v => Math.abs(Number(v) || 0) > 0);
  if (hasFlow) {
    const rows = [
      ['主力净流入', ff.main, ff.main_pct],
      ['超大单', ff.super_big, ff.super_big_pct],
      ['大单', ff.big, ff.big_pct],
      ['中单', ff.mid, ff.mid_pct],
      ['小单', ff.small, ff.small_pct],
    ];
    const vals = rows.map(r => Number(r[1]) || 0);
    const max = Math.max(1, ...vals.map(Math.abs));
    html += '<div class="sd-flow-title">资金流向（今日 · 红净流入 / 绿净流出）</div><div class="sd-flow">' + rows.map((r, k) => {
      const val = vals[k];
      const inflow = val >= 0;
      // 非零值给最小 6% 宽度，保证肉眼可见
      const w = val === 0 ? 0 : Math.max(6, Math.min(50, Math.abs(val) / max * 50));
      const cls = inflow ? 'up-c' : 'down-c';
      const p2 = Number(r[2]);
      const pct = (Number.isFinite(p2) && p2 !== 0) ? ` (${p2 >= 0 ? '+' : ''}${p2.toFixed(2)}%)` : '';
      // 用 class + data，颜色/定位插入后由 JS(CSSOM) 赋值（不受 CSP 拦截）
      return `<div class="sd-flow-row"><span class="fk">${r[0]}</span>
        <span class="fbar"><i class="fbar-fill" data-w="${w}" data-inflow="${inflow ? 1 : 0}"></i></span>
        <span class="fv ${cls}">${fmtAmt(val)}${pct}</span></div>`;
    }).join('') + '</div>';
  }
  el.innerHTML = html;
  // 资金流条上色：逐属性 CSSOM 赋值，绕过 CSP 对内联 style 的拦截
  el.querySelectorAll('.fbar-fill').forEach(fill => {
    const w = Number(fill.dataset.w) || 0;
    const inflow = fill.dataset.inflow === '1';
    const s = fill.style;
    s.position = 'absolute'; s.top = '0'; s.bottom = '0'; s.display = 'block';
    s[inflow ? 'left' : 'right'] = '50%';
    s.width = w + '%';
    s.background = inflow ? '#ff4d4f' : '#14c87d';
    s.borderRadius = '3px';
  });
}

export function showKline(code, name) {
  cur.code = code;
  cur.name = name || code.toUpperCase();
  document.getElementById('klineName').textContent = `${cur.name} · K线`;
  switchPage('kline');
  load();
  loadDetail(code);
}

export function initKline() {
  document.getElementById('klineBack').addEventListener('click', () => switchPage('watch'));
  document.getElementById('klineAi').addEventListener('click', aiAnalyze);
  document.querySelectorAll('.kp-btn').forEach(b => b.addEventListener('click', () => {
    document.querySelectorAll('.kp-btn').forEach(x => x.classList.toggle('active', x === b));
    cur.period = b.dataset.p;
    if (cur.code) load();
  }));

  const cv = document.getElementById('klineCanvas');

  // 滚轮缩放：以光标位置为锚点（上滚放大=更少蜡烛更宽，下滚缩小）
  cv.addEventListener('wheel', (e) => {
    if (!data.length) return;
    e.preventDefault();
    const rect = cv.getBoundingClientRect();
    const fx = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    const idxAtCursor = view.start + fx * view.count;
    const factor = e.deltaY > 0 ? 1.25 : 0.8;
    let nc = Math.round(view.count * factor);
    nc = Math.max(20, Math.min(data.length, nc));
    let ns = Math.round(idxAtCursor - fx * nc);
    ns = Math.max(0, Math.min(data.length - nc, ns));
    view = { start: ns, count: nc };
    draw();
  }, { passive: false });

  // 按住拖动平移
  let pan = null;
  cv.addEventListener('mousedown', (e) => { pan = { x: e.clientX, start: view.start }; });
  window.addEventListener('mousemove', (e) => {
    if (!pan || !data.length) return;
    const rect = cv.getBoundingClientRect();
    const dIdx = Math.round((pan.x - e.clientX) / rect.width * view.count);
    const ns = Math.max(0, Math.min(data.length - view.count, pan.start + dIdx));
    if (ns !== view.start) { view.start = ns; draw(); }
  });
  window.addEventListener('mouseup', () => { pan = null; });

  // ---- 触屏手势（移动端）：单指横扫平移、双指捏合缩放；单指竖扫不拦截（留给页面滚动）----
  let tpan = null;   // {x, y, start, axis:null|'x'|'y'}
  let pinch = null;  // {dist, fx, idxAtCenter, count}
  const dist2 = (a, b) => Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);

  cv.addEventListener('touchstart', (e) => {
    if (!data.length) return;
    if (e.touches.length === 2) {
      tpan = null;
      const [a, b] = e.touches;
      const rect = cv.getBoundingClientRect();
      const cx = (a.clientX + b.clientX) / 2;
      const fx = Math.min(1, Math.max(0, (cx - rect.left) / rect.width));
      pinch = { dist: dist2(a, b), fx, idxAtCenter: view.start + fx * view.count, count: view.count };
    } else if (e.touches.length === 1) {
      pinch = null;
      tpan = { x: e.touches[0].clientX, y: e.touches[0].clientY, start: view.start, axis: null };
    }
  }, { passive: false });

  cv.addEventListener('touchmove', (e) => {
    if (!data.length) return;
    const rect = cv.getBoundingClientRect();
    if (pinch && e.touches.length === 2) {
      e.preventDefault();
      const [a, b] = e.touches;
      const ratio = pinch.dist / Math.max(1, dist2(a, b)); // 张开→ratio<1→count减小→放大
      let nc = Math.round(pinch.count * ratio);
      nc = Math.max(20, Math.min(data.length, nc));
      let ns = Math.round(pinch.idxAtCenter - pinch.fx * nc);
      ns = Math.max(0, Math.min(data.length - nc, ns));
      view = { start: ns, count: nc };
      draw();
    } else if (tpan && e.touches.length === 1) {
      const tx = e.touches[0].clientX, ty = e.touches[0].clientY;
      if (tpan.axis === null) {
        const dx = Math.abs(tx - tpan.x), dy = Math.abs(ty - tpan.y);
        if (dx < 6 && dy < 6) return;        // 太小，先不判定方向
        tpan.axis = dx > dy ? 'x' : 'y';
      }
      if (tpan.axis === 'y') return;          // 竖扫：交给页面滚动
      e.preventDefault();                     // 横扫：平移K线
      const dIdx = Math.round((tpan.x - tx) / rect.width * view.count);
      const ns = Math.max(0, Math.min(data.length - view.count, tpan.start + dIdx));
      if (ns !== view.start) { view.start = ns; draw(); }
    }
  }, { passive: false });

  cv.addEventListener('touchend', (e) => {
    if (e.touches.length === 0) { tpan = null; pinch = null; }
    else if (e.touches.length === 1) {       // 双指退到单指：重置平移基准
      pinch = null;
      tpan = { x: e.touches[0].clientX, y: e.touches[0].clientY, start: view.start, axis: 'x' };
    }
  });

  if (!inTauri) console.log('[preview] K线走 mock 随机游走');
}

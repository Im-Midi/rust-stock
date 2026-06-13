// pages/kline.js — K线图（canvas 蜡烛图 + MA5/MA10 + 成交量）
// 交互：滚轮缩放（光标锚定）、按住拖动平移、日/周/月切换。自选股点名称进入。
import { fetchQuotes, fetchFundFlow, fetchFlowHistory, fetchStockBoards, fetchDividends, fetchShareInfo, fetchFinancials, fetchMargin, fetchLhbDetail, analyzeStock, predictKline } from '../api.js';
import { getKline } from '../klinecache.js';
import { calcChips } from '../chip.js';
import { indicatorSummary } from '../mytt.js';
import { switchPage } from '../router.js';
import { showAnalysis } from './analysis.js';
import { flashHint } from '../ui.js';
import { aiReady, storeGet, storeSet, today } from '../store.js';
import { inTauri, isMobile } from '../bridge.js';

const cur = { code: null, name: '', period: 'day' };
// 当前股的扩展维度（板块/概念、近5日主力净流入、分红、股本、核心财务、两融、龙虎榜），
// loadDetail 并行拉取填充、aiAnalyze 全部注入 AI 上下文
const extras = { code: null, boards: null, flows: null, divi: null, share: null, fin: null, margin: null, lhb: null };
let data = [];                      // 全量K线缓存（最多 250 根）
let view = { start: 0, count: 90 }; // 当前可视窗口
const FETCH_N = 250;

// AI 预测的下一交易日蜡烛（紫色叠加在最新真实蜡烛右侧）。
// 与产生它的 code+period 绑定；股票/周期变化或重新 load 时一律清空，避免串台。
let predicted = null;       // {open,high,low,close} 或 null
let predConf = '';          // 高/中/低
let predBasis = '';         // ≤40字依据
let predFor = '';           // 'code|period'，校验预测归属
function clearPredict() { predicted = null; predConf = ''; predBasis = ''; predFor = ''; const box = document.getElementById('klinePredict'); if (box) box.style.display = 'none'; }
const DISCLAIMER = '⚠️ AI 预测为模型基于历史数据的推演，极可能出错，仅供参考，绝不构成投资建议。';

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

  // 是否在最新真实蜡烛右侧追加一根 AI 预测蜡烛：必须有预测、且预测归属当前股+周期、且当前视图已展示到最后一根真实K线
  const showPred = predicted && predFor === (cur.code + '|' + cur.period) && (view.start + view.count >= data.length);
  let hi = Math.max(...candles.map(c => c.high));
  let lo = Math.min(...candles.map(c => c.low));
  if (showPred) { hi = Math.max(hi, predicted.high); lo = Math.min(lo, predicted.low); }
  const span = hi - lo || 1;
  const y = p => padT + (hi - p) / span * (priceH - padT);
  const maxVol = Math.max(...candles.map(c => c.volume)) || 1;

  const n = candles.length;
  const slots = n + (showPred ? 1 : 0); // 预测蜡烛额外占一个 x 槽位
  const step = plotW / slots;
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

  // —— AI 预测蜡烛（紫色虚线空心，纯叠加，不改真实蜡烛绘制）——
  if (showPred) {
    const PURPLE = '#9b5cff';
    const px = padL + n * step + step / 2;       // 紧贴最后一根真实蜡烛右侧的槽位
    const lastReal = candles[n - 1];
    const lastX = padL + (n - 1) * step + step / 2;
    // 虚线连接：最后一根真实收盘 → 预测收盘
    ctx.save();
    ctx.setLineDash([4, 3]);
    ctx.strokeStyle = PURPLE; ctx.lineWidth = 1.2;
    ctx.beginPath(); ctx.moveTo(lastX, y(lastReal.close)); ctx.lineTo(px, y(predicted.close)); ctx.stroke();
    // 预测蜡烛影线
    ctx.beginPath(); ctx.moveTo(px, y(predicted.high)); ctx.lineTo(px, y(predicted.low)); ctx.stroke();
    // 预测蜡烛实体：空心虚线框（区别于真实的实心红/绿）
    const yo = y(predicted.open), yc = y(predicted.close);
    const top = Math.min(yo, yc), hgt = Math.max(1.5, Math.abs(yo - yc));
    ctx.fillStyle = HOLLOW;
    ctx.fillRect(px - bw / 2, top, bw, hgt);
    ctx.lineWidth = 1.4;
    ctx.strokeRect(px - bw / 2, top, bw, hgt);
    ctx.restore();
    // 小标签「AI预测」
    ctx.save();
    ctx.fillStyle = PURPLE;
    ctx.font = '13px "DM Mono", monospace';
    ctx.textBaseline = 'bottom';
    const lbl = 'AI预测';
    let lx = px - ctx.measureText(lbl).width / 2;
    lx = Math.max(padL, Math.min(W - padR - ctx.measureText(lbl).width, lx));
    ctx.fillText(lbl, lx, Math.max(12, y(predicted.high) - 4));
    ctx.restore();
  }

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
  clearPredict(); // 重新加载K线 → 旧预测作废
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
      const meta = document.getElementById('klineMeta');
      meta.innerHTML =
        `${cur.code.toUpperCase()} · K线加载失败（网络波动，不会用模拟数据冒充真实价） ` +
        `<button id="klineRetry" class="kp-btn" type="button">↻ 重试</button>`;
      const rb = document.getElementById('klineRetry');
      if (rb) rb.addEventListener('click', () => {
        rb.disabled = true;
        load();              // 重新拉K线（成功路径会重写 klineMeta，按钮随之消失）
        loadDetail(cur.code); // 行情快照/资金流一并重试
      });
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

// 构建喂给 AI 的最丰富真实数据上下文（行情/筹码/指标/财务/资金流/龙虎榜…），解读与预测共用。
// 返回 {context, last, chg}；context 内含「上一收盘价 X」供后端涨跌停区间校验。
function buildContext() {
  const last = data[data.length - 1];
  const chg = last.open ? (last.close - last.open) / last.open * 100 : 0;
  const parts = [];
  parts.push(`${cur.name}（${cur.code.toUpperCase()}）现价 ${last.close.toFixed(2)}，今日 ${chg >= 0 ? '+' : ''}${chg.toFixed(2)}%`);
  parts.push(`上一收盘价 ${last.close.toFixed(2)}`); // 后端用此值做涨跌停区间校验
  const chip = calcChips(data, 80);
  if (chip) parts.push(`筹码分布：获利比例 ${(chip.profitRatio * 100).toFixed(1)}%，平均成本 ${chip.avgCost.toFixed(2)}，成本区间 ${chip.minPrice.toFixed(2)}–${chip.maxPrice.toFixed(2)}`);
  const ind = indicatorSummary(data);
  if (ind) {
    const cz = c => c === 'gold' ? '金叉' : c === 'dead' ? '死叉' : '无交叉';
    parts.push(`技术指标(通达信口径)：MACD ${cz(ind.macd.cross)}(DIF ${ind.macd.dif.toFixed(2)}/DEA ${ind.macd.dea.toFixed(2)})；KDJ ${cz(ind.kdj.cross)}(K${ind.kdj.k.toFixed(0)}/D${ind.kdj.d.toFixed(0)}/J${ind.kdj.j.toFixed(0)})；RSI6 ${ind.rsi.r6.toFixed(0)}；BOLL 上${ind.boll.up.toFixed(2)}/中${ind.boll.mid.toFixed(2)}/下${ind.boll.low.toFixed(2)}`);
  }
  if (data.length >= 20) { const a = data[data.length - 20].close; if (a) parts.push(`近20交易日累计 ${((last.close - a) / a * 100).toFixed(1)}%`); }
  // 扩展维度（loadDetail 已并行取好；与当前股不匹配时不注入，避免串台）
  if (extras.code === cur.code && extras.boards && extras.boards.length) {
    parts.push(`所属概念/板块: ${extras.boards.map(b => b.name).join('、')}`);
  }
  if (extras.code === cur.code && extras.flows && extras.flows.length) {
    const recent = extras.flows.slice(-5);
    const sum = recent.reduce((a, d) => a + d.main, 0);
    parts.push(`近${recent.length}日主力净流入(日): ${recent.map(d => `${d.date.slice(5)} ${fmtAmt(d.main)}`).join('，')}；合计 ${fmtAmt(sum)}`);
  }
  if (extras.code === cur.code && extras.share && extras.share.total_shares > 0) {
    const cap = last.close > 0 ? last.close * extras.share.total_shares : extras.share.total_cap;
    const peTxt = extras.share.pe ? `，动态市盈率 ${extras.share.pe.toFixed(1)}` : '';
    parts.push(`总市值约 ${fmtCap(cap)}（总股本 ${fmtShares(extras.share.total_shares)}，流通股 ${fmtShares(extras.share.float_shares)}）${peTxt}`);
  }
  if (extras.code === cur.code && extras.divi && extras.divi.length) {
    const d = extras.divi[0];
    const when = d.ex_date ? `除权日 ${d.ex_date}` : (d.progress || '待实施');
    const yld = d.yield_pct > 0.005 ? `，公告时点股息率 ${d.yield_pct.toFixed(2)}%` : '';
    parts.push(`最近分红: ${d.plan}（${when}${yld}）`);
  }
  if (extras.code === cur.code && extras.fin && extras.fin.report) {
    const f = extras.fin;
    const seg = [];
    if (f.revenue != null) seg.push(`营业收入 ${fmtFin(f.revenue)}${fmtYoy(f.revenue_yoy)}`);
    if (f.net_profit != null) seg.push(`归母净利润 ${fmtFin(f.net_profit)}${fmtYoy(f.net_profit_yoy)}`);
    if (f.eps != null) seg.push(`每股收益 ${f.eps.toFixed(2)}元`);
    if (f.roe != null) seg.push(`ROE ${f.roe.toFixed(2)}%`);
    if (f.gross_margin != null) seg.push(`毛利率 ${f.gross_margin.toFixed(1)}%`);
    if (f.debt_ratio != null) seg.push(`资产负债率 ${f.debt_ratio.toFixed(1)}%`);
    if (seg.length) parts.push(`核心财务指标(${f.report}): ${seg.join('，')}`);
  }
  if (extras.code === cur.code && extras.margin && extras.margin.length) {
    const m = extras.margin[0];
    const d = extras.margin.length > 1 ? `，融资余额较上日 ${fmtAmt(m.rzye - extras.margin[1].rzye)}` : '';
    parts.push(`融资融券(${m.date}): 融资余额 ${fmtFin(m.rzye)}，融券余额 ${fmtFin(m.rqye)}，两融合计 ${fmtFin(m.rzrqye)}${d}`);
  }
  if (extras.code === cur.code && extras.lhb && extras.lhb.length) {
    const d = extras.lhb[0];
    const more = extras.lhb.length > 1 ? `，近30日共上榜 ${extras.lhb.length} 次` : '';
    parts.push(`龙虎榜: ${d.date} 上榜（${d.reason}），席位净买额 ${fmtAmt(d.net)}（买入 ${fmtFin(d.buy)}/卖出 ${fmtFin(d.sell)}）${more}`);
  }
  const context = '截至最新交易日，本股真实数据如下：\n' + parts.join('\n');
  return { context, last, chg };
}

// AI 解读：把已算好的筹码/指标/行情作为真实数据注入 analyze_stock（数据注入层）
let aiBusy = false;
async function aiAnalyze() {
  if (!aiReady()) { flashHint('先在设置页接入 AI API Key'); return; }
  if (!data.length) { flashHint('K线未就绪'); return; }
  if (aiBusy) { flashHint('AI 正在解读，稍候'); return; }
  const { context, last, chg } = buildContext();
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

// AI 预测：把最丰富真实数据喂给 AI，推演下一交易日 OHLC，紫色蜡烛叠加到图上。
// 单根蜡烛的 LLM 预测准确度本就很低，结果仅作推演演示，全程保守措辞 + 显著免责声明。
let predBusy = false;
async function aiPredict() {
  if (!aiReady()) { flashHint('先在设置页接入 AI API Key'); return; }
  if (!data.length) { flashHint('K线未就绪'); return; }
  if (predBusy) { flashHint('AI 正在推演，稍候'); return; }
  if (cur.period !== 'day') { flashHint('预测仅支持日K口径'); return; }
  const { context } = buildContext();
  const forKey = cur.code + '|' + cur.period;
  predBusy = true;
  flashHint('AI 推演下一交易日…（仅供参考，非投资建议）');
  try {
    const res = await predictKline(cur.name, cur.code, context);
    if (cur.code + '|' + cur.period !== forKey) return; // 等待期间已切股/切周期，丢弃
    if (res && [res.open, res.high, res.low, res.close].every(v => typeof v === 'number' && isFinite(v) && v > 0)) {
      predicted = { open: res.open, high: res.high, low: res.low, close: res.close };
      predConf = res.confidence || '中';
      predBasis = res.basis || '';
      predFor = forKey;
      // 视图拉到最右，确保紫色预测蜡烛可见
      view.start = Math.max(0, data.length - view.count);
      draw();
      renderPredictBox();
    } else { flashHint('AI 未返回有效预测'); }
  } catch (e) { clearPredict(); flashHint('AI 预测失败：' + e); }
  finally { predBusy = false; }
}

// 预测结果文字框：依据 + 置信度 + 显著免责声明（DISCLAIMER 置顶醒目）
function renderPredictBox() {
  const box = document.getElementById('klinePredict');
  if (!box || !predicted) return;
  const f = v => v.toFixed(2);
  const chgPct = (() => { const lc = data[data.length - 1]; return lc && lc.close ? (predicted.close - lc.close) / lc.close * 100 : 0; })();
  const dir = chgPct >= 0 ? 'up-c' : 'down-c';
  const confCls = predConf === '高' ? 'up-c' : (predConf === '低' ? 'down-c' : '');
  box.innerHTML =
    `<div class="kp-disc">${DISCLAIMER}</div>` +
    `<div class="kp-row"><span style="color:#9b5cff">● AI 推演（下一交易日）</span> ` +
    `开 <b>${f(predicted.open)}</b> 高 <b>${f(predicted.high)}</b> 低 <b>${f(predicted.low)}</b> 收 <b class="${dir}">${f(predicted.close)}</b> ` +
    `<span class="${dir}">(${chgPct >= 0 ? '+' : ''}${chgPct.toFixed(2)}%)</span></div>` +
    `<div class="kp-row">置信度 <b class="${confCls}">${predConf}</b>${predBasis ? ' · 依据：' + predBasis : ''}</div>`;
  box.style.display = 'block';
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

// 所属板块/概念按日缓存（板块隶属变化极慢，SQLite 持久化，过期缓存可作离线兜底）
async function loadBoards(code) {
  const key = 'boards_' + code;
  const cached = await storeGet(key, null);
  const valid = cached && Array.isArray(cached.boards) && cached.boards.length;
  if (valid && cached.date === today()) return cached.boards;
  const fresh = await fetchStockBoards(code);
  if (fresh) { storeSet(key, { date: today(), boards: fresh }); return fresh; }
  return valid ? cached.boards : null; // 拉取失败 → 沿用旧缓存（绝不造假，没有就不显示）
}

// 市值（元）→ 万亿/亿；股本（股）→ 亿股/万股。全部四舍五入后展示
function fmtCap(yuan) {
  if (!(yuan > 0)) return '--';
  if (yuan >= 1e12) return (yuan / 1e12).toFixed(2) + '万亿';
  const yi = yuan / 1e8;
  return (yi >= 100 ? yi.toFixed(0) : yi.toFixed(1)) + '亿';
}
function fmtShares(shares) {
  if (!(shares > 0)) return '--';
  if (shares >= 1e8) return (shares / 1e8).toFixed(2) + '亿股';
  return (shares / 1e4).toFixed(0) + '万股';
}

// 分红历史按日缓存（年度级数据；[] = 该股确实从未分红，也按日缓存，避免反复打接口）
async function loadDividends(code) {
  const key = 'divi_' + code;
  const cached = await storeGet(key, null);
  const valid = cached && Array.isArray(cached.list);
  if (valid && cached.date === today()) return cached.list;
  const fresh = await fetchDividends(code); // [] 合法（从未分红），null 才是失败
  if (fresh) { storeSet(key, { date: today(), list: fresh }); return fresh; }
  return valid ? cached.list : null; // 失败 → 沿用旧缓存；没有就不显示（绝不造假）
}

// 股本/市值按日缓存（股本年内基本不变；市值展示时用最新现价 × 股本 现算保持实时）
async function loadShareInfo(code) {
  const key = 'share_' + code;
  const cached = await storeGet(key, null);
  const valid = cached && cached.info && cached.info.total_shares > 0;
  if (valid && cached.date === today()) return cached.info;
  const fresh = await fetchShareInfo(code);
  if (fresh && fresh.total_shares > 0) { storeSet(key, { date: today(), info: fresh }); return fresh; }
  return valid ? cached.info : null;
}

// 财务金额（元）→ 万亿/亿/万（保留负号；净利可为负。null/非数 → "--"）
function fmtFin(yuan) {
  if (yuan == null || !Number.isFinite(yuan)) return '--';
  const sign = yuan < 0 ? '-' : '';
  const a = Math.abs(yuan);
  if (a >= 1e12) return sign + (a / 1e12).toFixed(2) + '万亿';
  if (a >= 1e8) return sign + (a / 1e8).toFixed(1) + '亿';
  if (a >= 1e4) return sign + (a / 1e4).toFixed(0) + '万';
  return sign + a.toFixed(0);
}
// 同比 % → "(+6.3%)"；未披露(null) → 空串（不造 0）
function fmtYoy(v) {
  return (v == null || !Number.isFinite(v)) ? '' : `(${v >= 0 ? '+' : ''}${v.toFixed(1)}%)`;
}
// 龙虎榜"近期"判定：上榜日距今 ≤30 天才展示/注入 AI（多年前的老黄历不提）
function recentLhb(list) {
  if (!Array.isArray(list) || !list.length) return [];
  const cutoff = Date.now() - 30 * 86400000;
  return list.filter(it => {
    const t = new Date(it.date + 'T00:00:00').getTime();
    return Number.isFinite(t) && t >= cutoff;
  });
}
// 上榜原因瘦身：去掉"非ST、*ST和S证券"模板前缀，超长截断（小屏一行放不下全文）
function shortReason(r) {
  const s = (r || '').replace(/^非ST、\*ST和S证券/, '');
  return s.length > 20 ? s.slice(0, 20) + '…' : s;
}

// 核心财务指标按日缓存（季报级数据；拉取失败 → 沿用旧缓存，没有就不显示，绝不造假）
async function loadFinancials(code) {
  const key = 'fin_' + code;
  const cached = await storeGet(key, null);
  const valid = cached && cached.fin && cached.fin.report;
  if (valid && cached.date === today()) return cached.fin;
  const fresh = await fetchFinancials(code);
  if (fresh && fresh.report) { storeSet(key, { date: today(), fin: fresh }); return fresh; }
  return valid ? cached.fin : null;
}

// 两融余额按日缓存（交易所按日披露；[] = 非两融标的，也按日缓存避免反复打接口）
async function loadMargin(code) {
  const key = 'margin_' + code;
  const cached = await storeGet(key, null);
  const valid = cached && Array.isArray(cached.list);
  if (valid && cached.date === today()) return cached.list;
  const fresh = await fetchMargin(code); // [] 合法（非两融标的），null 才是失败
  if (fresh) { storeSet(key, { date: today(), list: fresh }); return fresh; }
  return valid ? cached.list : null;
}

// 龙虎榜明细按日缓存（按日披露；[] = 从未上榜，也按日缓存）
async function loadLhb(code) {
  const key = 'lhbd_' + code;
  const cached = await storeGet(key, null);
  const valid = cached && Array.isArray(cached.list);
  if (valid && cached.date === today()) return cached.list;
  const fresh = await fetchLhbDetail(code); // [] 合法（从未上榜），null 才是失败
  if (fresh) { storeSet(key, { date: today(), list: fresh }); return fresh; }
  return valid ? cached.list : null;
}

async function loadDetail(code) {
  const el = document.getElementById('stockDetail');
  el.innerHTML = '';
  // 并行取行情快照 + 资金流（换手率随资金流接口一起来）+ 近5日主力净流入 + 所属板块
  const [quotes, ff, flows, boards, divi, share, fin, margin, lhbAll] = await Promise.all([
    fetchQuotes([code]), fetchFundFlow(code), fetchFlowHistory(code, 5), loadBoards(code),
    loadDividends(code), loadShareInfo(code),
    loadFinancials(code), loadMargin(code), loadLhb(code),
  ]);
  if (code !== cur.code) return; // 等待期间已切到别的股票，丢弃过期渲染
  extras.code = code; extras.flows = flows; extras.boards = boards;
  extras.divi = divi; extras.share = share;
  extras.fin = fin; extras.margin = margin; extras.lhb = recentLhb(lhbAll);
  const q = quotes && quotes[0];
  let html = '';
  // 所属板块/概念 chips（行业高亮）
  if (boards && boards.length) {
    html += '<div class="sd-chips">' + boards.map(b =>
      `<span class="sd-chip${b.kind === '行业' ? ' ind' : ''}">${b.name}</span>`).join('') + '</div>';
  }
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
    // 股本/市值（东财 stock/get，按日缓存）：市值用最新现价 × 股本现算保持实时；
    // 接口失败且无缓存 → 不补这 6 格（绝不造假）
    if (share && share.total_shares > 0) {
      const totalCap = q.price > 0 ? q.price * share.total_shares : share.total_cap;
      const floatCap = q.price > 0 ? q.price * share.float_shares : share.float_cap;
      cells.push(
        ['总市值', fmtCap(totalCap), ''],
        ['流通市值', fmtCap(floatCap), ''],
        ['总股本', fmtShares(share.total_shares), ''],
        ['流通股', fmtShares(share.float_shares), ''],
        ['市盈(动)', share.pe ? share.pe.toFixed(2) : '--', share.pe < 0 ? 'down-c' : ''],
        ['市净率', share.pb > 0 ? share.pb.toFixed(2) : '--', ''],
      );
    }
    html += '<div class="sd-grid">' + cells.map(c =>
      `<div class="sd-cell"><div class="k">${c[0]}</div><div class="v ${c[2]}">${c[1]}</div></div>`).join('') + '</div>';
  }
  // 核心财务指标一行（最新报告期；未披露字段省略；失败且无缓存 → 整行隐藏，绝不造假）
  if (fin && fin.report) {
    const seg = [];
    if (fin.revenue != null) seg.push(`营收 ${fmtFin(fin.revenue)}${fmtYoy(fin.revenue_yoy)}`);
    if (fin.net_profit != null) seg.push(`净利 ${fmtFin(fin.net_profit)}${fmtYoy(fin.net_profit_yoy)}`);
    if (fin.eps != null) seg.push(`EPS ${fin.eps.toFixed(2)}`);
    if (fin.roe != null) seg.push(`ROE ${fin.roe.toFixed(1)}%`);
    if (fin.gross_margin != null) seg.push(`毛利 ${fin.gross_margin.toFixed(1)}%`);
    if (fin.debt_ratio != null) seg.push(`负债 ${fin.debt_ratio.toFixed(1)}%`);
    if (seg.length) html += `<div class="sd-divi">财报 <b>${fin.report}</b> · ${seg.join(' · ')}</div>`;
  }
  // 融资融券一行（最新交易日余额 + 融资余额较上日增减；非两融标的([])或失败无缓存 → 隐藏）
  if (margin && margin.length) {
    const m = margin[0];
    const d = margin.length > 1 ? m.rzye - margin[1].rzye : null;
    const delta = d == null ? '' : ` <span class="${d >= 0 ? 'up-c' : 'down-c'}">${fmtAmt(d)}</span>`;
    html += `<div class="sd-divi">两融(${m.date.slice(5)}) 融资余额 <b>${fmtFin(m.rzye)}</b>${delta} · 融券 ${fmtFin(m.rqye)} · 合计 ${fmtFin(m.rzrqye)}</div>`;
  }
  // 龙虎榜一行（仅近 30 日上过榜才显示；从未上榜 / 久未上榜 → 整行不出现）
  if (extras.lhb.length) {
    const d = extras.lhb[0];
    const cls = d.net >= 0 ? 'up-c' : 'down-c';
    const more = extras.lhb.length > 1 ? ` · 近30日${extras.lhb.length}次` : '';
    html += `<div class="sd-divi">龙虎榜 <b>${d.date.slice(5)}</b> 净买 <span class="${cls}">${fmtAmt(d.net)}</span> · ${shortReason(d.reason)}${more}</div>`;
  }
  // 最近分红（东财分红明细，按日缓存；从未分红或拉取失败都不显示该行）
  if (divi && divi.length) {
    const d = divi[0];
    const when = d.ex_date ? `除权 ${d.ex_date}` : (d.progress || '待实施');
    const yld = d.yield_pct > 0.005 ? ` · 股息率 ${d.yield_pct.toFixed(2)}%` : '';
    html += `<div class="sd-divi">最近分红 <b>${d.plan}</b> · ${when}${yld}</div>`;
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
  // 近5日主力净流入（日级历史，红流入/绿流出；接口升序旧→新，取尾部 5 根）
  if (flows && flows.length) {
    const recent = flows.slice(-5);
    html += '<div class="sd-flow-title">近5日主力净流入（日）</div><div class="sd-hist">' +
      recent.map(d => {
        const cls = d.main >= 0 ? 'up-c' : 'down-c';
        return `<div class="sd-hist-cell"><div class="d">${d.date.slice(5)}</div><div class="v ${cls}">${fmtAmt(d.main)}</div></div>`;
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
  clearPredict(); // 切股 → 清旧预测
  document.getElementById('klineName').textContent = `${cur.name} · K线`;
  switchPage('kline');
  load();
  loadDetail(code);
}

export function initKline() {
  document.getElementById('klineBack').addEventListener('click', () => switchPage('watch'));
  document.getElementById('klineAi').addEventListener('click', aiPredict);
  document.querySelectorAll('.kp-btn').forEach(b => b.addEventListener('click', () => {
    document.querySelectorAll('.kp-btn').forEach(x => x.classList.toggle('active', x === b));
    cur.period = b.dataset.p;
    clearPredict(); // 切周期 → 清旧预测
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

// pages/recommend.js — 今日 AI 推荐：详尽分析后推荐 3 支
// 历史按日存 SQLite（rec_history），连续 ≥7 个推荐日出现同一支 → ★ 标识并注明天数
import { aiRecommend, fetchQuotes } from '../api.js';
import { getKline, mapLimit } from '../klinecache.js';
import { state, saveRecHistory, saveWatch, today, aiReady } from '../store.js';
import { flashHint } from '../ui.js';
import { inTauri } from '../bridge.js';
import { showAnalysis } from './analysis.js';
import { showKline } from './kline.js';
import { getSentiment } from './market.js';

const mockRecs = [
  { code: 'sh600519', name: '贵州茅台', score: 72, change_pct: 1.85, reason: '（浏览器预览示例）高端白酒需求韧性强，渠道动销回暖，估值处历史中位；技术面突破年线。风险：消费复苏不及预期。' },
  { code: 'sz300750', name: '宁德时代', score: 55, change_pct: 0.42, reason: '（浏览器预览示例）动力电池份额稳固，储能业务高增；近期回调后估值合理。风险：行业价格战。' },
  { code: 'sh688041', name: '海光信息', score: 48, change_pct: -0.31, reason: '（浏览器预览示例）国产算力需求旺盛，订单能见度高。风险：供应链与估值波动。' },
];

let pending = false;
let recPrices = {}; // code -> {price,change_pct} 实时行情(独立于候选池)

// 连续 6 个交易日「逐日下行」才剔除：要求最近 6 个交易日，每一天的收盘价
// 都严格低于前一天（6 连阴式下跌）。判定需要 7 个收盘价（= 6 个日间变动）：
//   7 个收盘价两两相邻全部「后一天 < 前一天」→ 连续6日下行 → 剔除；
//   只要中间有任意一天没跌（持平或上涨）→ 不算连续下行 → 保留。
// 数据：基于真实日 K 收盘价；不足 7 个收盘价无法判定连续6日 → 返回 false（不剔除）。
function is6dDown(candles) {
  const closes = (candles || []).map(c => c.close).filter(x => typeof x === 'number');
  if (closes.length < 7) return false;
  const w = closes.slice(-7); // 最近 7 个收盘 → 6 个日间变动
  for (let i = 1; i < w.length; i++) {
    if (!(w[i] < w[i - 1])) return false; // 有一天没跌 → 不算连续6日下行
  }
  return true; // 连续 6 日每天都比前一天低
}

// 连续推荐天数：从最近的推荐日往前数，必须每个推荐日都包含该股
function streakOf(code) {
  const days = Object.keys(state.recHistory).sort().reverse(); // 新→旧
  let n = 0;
  for (const d of days) {
    const list = state.recHistory[d] || [];
    if (list.some(r => r.code === code)) n++;
    else break;
  }
  return n;
}

function setRefreshBtn(busy) {
  const b = document.getElementById('recRefresh');
  b.textContent = busy ? '⏳ 生成中…' : '↻ 重新生成';
  b.disabled = busy;
}

async function generate(force = false, manual = false) {
  // manual=true 是用户点按钮，给出明确反馈；auto 模式保持安静
  if (!inTauri) { if (manual) flashHint('浏览器预览无法调用 AI'); return; }
  if (!aiReady()) { if (manual) flashHint('先在设置页接入 AI API Key'); return; }
  if (pending) { if (manual) flashHint('AI 正在生成中，约需 3~6 分钟（候选池初选 + 对每支入选股逐股深度调研），请稍候'); return; }
  const tk = today();
  if (!force && state.recHistory[tk]) return;
  pending = true;
  // 立即清掉旧结果（保留备份，失败时恢复），让界面马上切到"生成中"状态
  const backup = state.recHistory[tk];
  delete state.recHistory[tk];
  setRefreshBtn(true);
  renderRecommend(); // 显示"生成中"
  let ok = false;
  let errMsg = '';
  try {
    const s = getSentiment();
    const ctx = s
      ? `今天是 ${tk}，A股市场情绪 ${s.label}（${s.score} 分），主要指数：${(s.components || []).map(c => `${c.name} ${c.change_pct >= 0 ? '+' : ''}${c.change_pct.toFixed(2)}%`).join('，')}`
      : `今天是 ${tk}`;
    const recs = await aiRecommend(ctx);
    if (Array.isArray(recs) && recs.length) {
      state.recHistory[tk] = recs;
      ok = true;
      const days = Object.keys(state.recHistory).sort().reverse();
      for (const d of days.slice(30)) delete state.recHistory[d];
      saveRecHistory();
    } else {
      errMsg = 'AI 未返回有效推荐，请重试';
    }
  } catch (e) {
    errMsg = String(e).replace(/^Error:\s*/, '');
    console.warn('AI 推荐失败:', e);
  } finally {
    if (!ok && backup) state.recHistory[tk] = backup; // 失败恢复旧结果
    pending = false;
    setRefreshBtn(false);
    renderRecommend();
    if (!ok && errMsg) {
      if (!state.recHistory[tk]) {
        // 无旧结果可显示 → 错误占位（可换行）
        document.getElementById('recList').innerHTML =
          `<div class="rec-empty err-text">⚠️ ${errMsg}</div>`;
      } else {
        flashHint('AI 推荐失败，已保留上次结果');
      }
    }
  }
}

export function renderRecommend(skipFill) {
  const list = document.getElementById('recList');
  const note = document.getElementById('recNote');
  const meta = document.getElementById('recMeta');
  const tk = today();
  let recs = state.recHistory[tk];

  if (!inTauri) recs = mockRecs; // 浏览器预览

  meta.textContent = recs ? tk : '';
  if (!recs) {
    if (pending) {
      list.innerHTML = '<div class="rec-empty">⏳ AI 正在初选候选池并对每支入选股逐股做产业链深度调研，约需 3~6 分钟…</div>';
      note.textContent = '';
    } else if (!aiReady()) {
      list.innerHTML = '<div class="rec-empty">接入 AI（设置页）后，每天自动生成 3 支推荐</div>';
      note.textContent = '';
    } else {
      list.innerHTML = '<div class="rec-empty">今日推荐待生成</div>';
      note.textContent = '';
    }
    return;
  }

  // 近6日收盘价折线下行 → 剔除（需已缓存日K才能判断；未缓存先全展示，拉到K线后复筛）
  const hidden = new Set(recs.filter(r => sparkCache[r.code] && is6dDown(sparkCache[r.code])).map(r => r.code));
  if (recs.length && hidden.size === recs.length) {
    list.innerHTML = '<div class="rec-empty">今日候选均连续6日逐日下行，已按规则全部剔除</div>';
    note.textContent = '';
    return;
  }

  list.innerHTML = recs.map((r, i) => {
    if (hidden.has(r.code)) return '';
    const streak = inTauri ? streakOf(r.code) : (i === 0 ? 8 : 1); // 预览演示星标
    const starred = streak >= 7;
    const up = r.score >= 0;
    const inWl = state.watchlist.includes(r.code);
    const lp = recPrices[r.code];
    const price = lp ? lp.price : r.price;
    const chg = lp ? lp.change_pct : (r.change_pct || 0);
    return `<div class="rec-row" data-i="${i}">
      <div class="r-rank">${i + 1}</div>
      <div class="r-name">
        <b>${starred ? '<span class="star">★</span> ' : ''}${r.name}${starred ? `<span class="r-streak">已连续 ${streak} 日推荐</span>` : ''}</b>
        <i>${r.code.toUpperCase()}</i>
      </div>
      <div class="r-right">
        <div class="r-score ${up ? 'up-c' : 'down-c'}">${up ? '+' : ''}${r.score}</div>
        ${price
          ? `<div class="r-chg ${chg >= 0 ? 'up-c' : 'down-c'}">现价 ${price.toFixed(2)}　${chg >= 0 ? '+' : ''}${chg.toFixed(2)}%</div>`
          : `<div class="r-chg" style="color:var(--txt-3)">现价加载中…</div>`}
      </div>
      <canvas class="rec-spark" width="108" height="56" data-spark="${i}" title="点击看K线"></canvas>
      <button class="rec-add${inWl ? ' added' : ''}" data-add="${i}" title="${inWl ? '已在自选' : '一键加入自选'}">${inWl ? '✓' : '＋'}</button>
    </div>`;
  }).join('');
  note.innerHTML = '右侧缩略图＝<span class="spark-label">近30日收盘价折线</span>（真实日K收盘价连线，点击看完整日K）。本地筛全市场候选池(涨幅/主力净流入/龙虎榜) → AI 用供应链瓶颈+多流派(价值/成长/游资/技术/宏观)+龙虎榜初选，再对每支入选股做单股产业链深度调研（与「研」同源）。连续6日收盘价逐日下行（6连阴）者已自动剔除。★=连续≥7推荐日同股。仅供参考，不构成投资建议。';
  drawSparks(recs);
  if (!skipFill && inTauri) { fillRecPrices(recs); ensureSparkData(recs); }
}

// 给每支推荐拉真实实时行情，显示现价（独立于会失败的候选池）
async function fillRecPrices(recs) {
  const codes = [...new Set(recs.map(r => r.code))];
  if (!codes.length) return;
  const quotes = await fetchQuotes(codes);
  if (!quotes || !quotes.length) return;
  quotes.forEach(q => { recPrices[q.code] = { price: q.price, change_pct: q.change_pct }; });
  renderRecommend(true);
}

// ---------- 每支推荐的真实近30日「收盘价折线」缩略图（点击进完整日K）----------
const sparkCache = {}; // code -> 真实日K数组（会话内缓存，仅取收盘价连线）
// 仅从缓存绘制可见行的缩略图（不发请求）
function drawSparks(recs) {
  for (let i = 0; i < recs.length; i++) {
    const cv = document.querySelector(`canvas.rec-spark[data-spark="${i}"]`);
    if (!cv) continue;
    const candles = sparkCache[recs[i].code];
    if (candles) drawSpark(cv, candles);
  }
}
// 拉齐所有推荐的近30日日K（无论是否显示，用于近6日下行复筛）。
// 走共享K线缓存(klinecache)：有界并发 3 支（此前是串行 for-await，手机上一支卡住
// 整队排队几分钟）；每支到货立即画该支缩略图（渐进渲染，不等全队列）；
// 失败的股票 30s 冷却后自动重试（最多 4 轮），不再"一次失败永远空白"。
let sparkLoading = false;
const sparkFailAt = {}; // code -> 上次失败时间戳（30s 冷却）
let sparkRetryLeft = 4; // 自动重试轮数上限（每次手动/路由触发会重置）
async function ensureSparkData(recs, isRetry) {
  if (sparkLoading) return;
  if (!isRetry) sparkRetryLeft = 4;
  const now = Date.now();
  const todo = recs.filter(r => !sparkCache[r.code] && now - (sparkFailAt[r.code] || 0) > 30000);
  if (!todo.length) return;
  sparkLoading = true;
  try {
    await mapLimit(todo, 3, async (r) => {
      const res = await getKline(r.code, 'day', 30);
      if (res && res.candles.length) {
        sparkCache[r.code] = res.candles;
        delete sparkFailAt[r.code];
        // 渐进渲染：到一支画一支（data-spark 用完整 recs 索引，与 DOM 对齐）
        const i = recs.indexOf(r);
        const cv = document.querySelector(`canvas.rec-spark[data-spark="${i}"]`);
        if (cv) drawSpark(cv, res.candles);
      } else {
        sparkFailAt[r.code] = Date.now();
      }
    });
  } finally { sparkLoading = false; }
  renderRecommend(true); // 缓存齐 → 复筛(6连阴剔除) + 重画
  // 还有失败的 → 冷却后安静地再试一轮（上限 4 轮，防无网时无限轮询）
  if (sparkRetryLeft > 0 && recs.some(r => !sparkCache[r.code])) {
    sparkRetryLeft--;
    setTimeout(() => { ensureSparkData(recs, true); }, 31000);
  }
}
// 近 30 日「收盘价折线」（真实日K的收盘价连线，红涨绿跌）。点击进完整日K。
function drawSpark(cv, candles) {
  const ctx = cv.getContext('2d');
  const W = cv.width, H = cv.height;
  ctx.clearRect(0, 0, W, H);
  const closes = candles.map(c => c.close);
  if (closes.length < 2) return;
  const hi = Math.max(...closes), lo = Math.min(...closes), span = hi - lo || 1;
  const up = closes[closes.length - 1] >= closes[0];
  const cs = getComputedStyle(document.body);
  const color = up ? (cs.getPropertyValue('--up').trim() || '#ff4d4f') : (cs.getPropertyValue('--down').trim() || '#14c87d');
  const pad = 4;
  const xy = (c, i) => [pad + i / (closes.length - 1) * (W - pad * 2), H - pad - (c - lo) / span * (H - pad * 2)];
  // 渐变填充
  ctx.beginPath();
  closes.forEach((c, i) => { const [x, y] = xy(c, i); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
  const [lx] = xy(closes[closes.length - 1], closes.length - 1);
  ctx.lineTo(lx, H); ctx.lineTo(pad, H); ctx.closePath();
  ctx.fillStyle = up ? 'rgba(255,77,79,.14)' : 'rgba(20,200,125,.14)';
  ctx.fill();
  // 折线
  ctx.beginPath();
  closes.forEach((c, i) => { const [x, y] = xy(c, i); i ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
  ctx.strokeStyle = color; ctx.lineWidth = 2; ctx.lineJoin = 'round'; ctx.stroke();
}

function addToWatch(i) {
  const recs = inTauri ? state.recHistory[today()] : mockRecs;
  const r = recs && recs[i];
  if (!r) return;
  if (state.watchlist.includes(r.code)) { flashHint('已在自选里了'); return; }
  state.watchlist.push(r.code);
  saveWatch();
  flashHint(`已加入自选：${r.name}`);
  renderRecommend(); // 按钮变 ✓
}

// ---------- 推荐战绩：用K线回算每次推荐的后续表现 ----------
let perfCache = null;   // 本次会话算过就复用
let perfOpen = false;
let perfBusy = false;

async function computePerf() {
  const days = Object.keys(state.recHistory).sort().reverse().filter(d => d !== today()).slice(0, 10);
  const rows = [];
  for (const d of days) for (const r of (state.recHistory[d] || [])) rows.push({ day: d, code: r.code, name: r.name, score: r.score });
  if (!rows.length) return { rows: [], winRate: 0, avg: 0 };
  const codes = [...new Set(rows.map(r => r.code))];
  const klines = {};
  // 共享K线缓存 + 有界并发 4（此前无界 Promise.all，几十支同时打接口易触发掐线）
  const ks = await mapLimit(codes, 4, c => getKline(c, 'day', 60));
  codes.forEach((c, i) => { klines[c] = ks[i] && ks[i].candles; });
  const out = [];
  for (const r of rows) {
    const k = klines[r.code];
    if (!k || !k.length) continue;
    // 推荐日（或其后第一个交易日）的收盘 → 最新收盘
    const base = k.find(c => c.date >= r.day);
    const last = k[k.length - 1];
    if (!base || !last || base.date === last.date || !base.close) continue;
    out.push({ ...r, ret: (last.close - base.close) / base.close * 100 });
  }
  const wins = out.filter(r => r.ret > 0).length;
  return {
    rows: out,
    winRate: out.length ? wins / out.length * 100 : 0,
    wins,
    avg: out.length ? out.reduce((a, r) => a + r.ret, 0) / out.length : 0,
  };
}

function renderPerf(p) {
  const el = document.getElementById('recPerfPanel');
  if (!p.rows.length) {
    el.innerHTML = '<div class="rec-empty">还没有可回算的历史推荐（需要往日推荐 + 至少一个交易日）</div>';
    return;
  }
  const sum = `<div class="perf-sum">胜率 <b class="${p.winRate >= 50 ? 'up-c' : 'down-c'}">${p.winRate.toFixed(0)}%</b>（${p.wins}/${p.rows.length}） · 平均 <b class="${p.avg >= 0 ? 'up-c' : 'down-c'}">${p.avg >= 0 ? '+' : ''}${p.avg.toFixed(2)}%</b> · 推荐日收盘 → 最新收盘</div>`;
  el.innerHTML = sum + p.rows.slice(0, 15).map(r => `
    <div class="perf-row">
      <span class="p-day">${r.day.slice(5)}</span>
      <span class="p-name">${r.name}</span>
      <span class="p-score">${r.score > 0 ? '+' : ''}${r.score}</span>
      <span class="p-ret ${r.ret >= 0 ? 'up-c' : 'down-c'}">${r.ret >= 0 ? '+' : ''}${r.ret.toFixed(2)}%</span>
    </div>`).join('');
}

async function togglePerf() {
  const el = document.getElementById('recPerfPanel');
  perfOpen = !perfOpen;
  el.style.display = perfOpen ? 'block' : 'none';
  if (!perfOpen || perfBusy) return;
  if (perfCache) { renderPerf(perfCache); return; }
  if (!inTauri) {
    renderPerf({ rows: [
      { day: '2026-06-04', name: '贵州茅台', score: 72, ret: 4.12 },
      { day: '2026-06-04', name: '宁德时代', score: 55, ret: -1.30 },
      { day: '2026-06-03', name: '中际旭创', score: 64, ret: 7.85 },
    ], winRate: 66.7, wins: 2, avg: 3.56 });
    return;
  }
  perfBusy = true;
  el.innerHTML = '<div class="rec-empty">⏳ 正在用K线回算历史推荐表现…</div>';
  try {
    perfCache = await computePerf();
    renderPerf(perfCache);
  } catch (e) {
    el.innerHTML = '<div class="rec-empty">回算失败：' + e + '</div>';
  } finally {
    perfBusy = false;
  }
}

export function initRecommend() {
  document.getElementById('recPerf').addEventListener('click', togglePerf);
  document.getElementById('recList').addEventListener('click', (e) => {
    // 一键加自选（在行点击之前拦截）
    const addBtn = e.target.closest('.rec-add');
    if (addBtn) { addToWatch(+addBtn.dataset.add); return; }
    const spark = e.target.closest('.rec-spark');
    if (spark) {
      const recsK = inTauri ? state.recHistory[today()] : mockRecs;
      const rk = recsK && recsK[+spark.dataset.spark];
      if (rk) showKline(rk.code, rk.name);
      return;
    }
    const row = e.target.closest('.rec-row');
    if (!row) return;
    const tk = today();
    const recs = inTauri ? state.recHistory[tk] : mockRecs;
    const r = recs && recs[+row.dataset.i];
    if (!r) return;
    const streak = inTauri ? streakOf(r.code) : 0;
    showAnalysis({
      title: `${r.name} · AI 推荐`,
      score: r.score,
      text: r.reason,
      meta: `${r.code.toUpperCase()} · 今日推荐${streak >= 7 ? ` · ★ 已连续 ${streak} 日` : ''} · 仅供参考，不构成投资建议`,
      back: 'market',
    });
  });
  document.getElementById('recRefresh').addEventListener('click', () => generate(true, true));
  // 启动后自动生成（当天没有才生成）
  generate(false);
}

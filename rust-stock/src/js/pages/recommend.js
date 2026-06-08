// pages/recommend.js — 今日 AI 推荐：详尽分析后推荐 3 支
// 历史按日存 SQLite（rec_history），连续 ≥7 个推荐日出现同一支 → ★ 标识并注明天数
import { aiRecommend, fetchKline } from '../api.js';
import { state, saveRecHistory, saveWatch, today, aiReady } from '../store.js';
import { flashHint } from '../ui.js';
import { inTauri } from '../bridge.js';
import { showAnalysis } from './analysis.js';
import { getSentiment } from './market.js';

const mockRecs = [
  { code: 'sh600519', name: '贵州茅台', score: 72, change_pct: 1.85, reason: '（浏览器预览示例）高端白酒需求韧性强，渠道动销回暖，估值处历史中位；技术面突破年线。风险：消费复苏不及预期。' },
  { code: 'sz300750', name: '宁德时代', score: 55, change_pct: 0.42, reason: '（浏览器预览示例）动力电池份额稳固，储能业务高增；近期回调后估值合理。风险：行业价格战。' },
  { code: 'sh688041', name: '海光信息', score: 48, change_pct: -0.31, reason: '（浏览器预览示例）国产算力需求旺盛，订单能见度高。风险：供应链与估值波动。' },
];

let pending = false;

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
  if (pending) { if (manual) flashHint('AI 正在生成中，约需 1~2 分钟（详尽产业链分析），请稍候'); return; }
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

export function renderRecommend() {
  const list = document.getElementById('recList');
  const note = document.getElementById('recNote');
  const meta = document.getElementById('recMeta');
  const tk = today();
  let recs = state.recHistory[tk];

  if (!inTauri) recs = mockRecs; // 浏览器预览

  meta.textContent = recs ? tk : '';
  if (!recs) {
    if (pending) {
      list.innerHTML = '<div class="rec-empty">⏳ AI 正在详尽分析今日盘面并用实时行情复核，约需 1~2 分钟（详尽产业链分析）…</div>';
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

  list.innerHTML = recs.map((r, i) => {
    const streak = inTauri ? streakOf(r.code) : (i === 0 ? 8 : 1); // 预览演示星标
    const starred = streak >= 7;
    const up = r.score >= 0;
    const inWl = state.watchlist.includes(r.code);
    return `<div class="rec-row" data-i="${i}">
      <div class="r-rank">${i + 1}</div>
      <div class="r-name">
        <b>${starred ? '<span class="star">★</span> ' : ''}${r.name}${starred ? `<span class="r-streak">已连续 ${streak} 日推荐</span>` : ''}</b>
        <i>${r.code.toUpperCase()}</i>
      </div>
      <div class="r-right">
        <div class="r-score ${up ? 'up-c' : 'down-c'}">${up ? '+' : ''}${r.score}</div>
        ${typeof r.change_pct === 'number' && r.price !== 0
          ? `<div class="r-chg ${r.change_pct >= 0 ? 'up-c' : 'down-c'}">今日 ${r.change_pct >= 0 ? '+' : ''}${r.change_pct.toFixed(2)}%</div>`
          : ''}
      </div>
      <button class="rec-add${inWl ? ' added' : ''}" data-add="${i}" title="${inWl ? '已在自选' : '一键加入自选'}">${inWl ? '✓' : '＋'}</button>
    </div>`;
  }).join('');
  note.textContent = 'AI 候选已用实时行情复核（当日明显下跌/查无此码的自动淘汰）。★ = 连续一周（≥7 个推荐日）推荐同一支。仅供参考，不构成投资建议。';
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
  await Promise.all(codes.map(async c => { klines[c] = await fetchKline(c, 'day', 60); }));
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

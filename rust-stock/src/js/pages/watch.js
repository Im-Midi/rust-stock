// pages/watch.js — 自选股：增删、行情刷新、AI 看涨/看跌小仪表盘、分析详情页
import { fetchQuotes, normalizeCode, analyzeStock } from '../api.js';
import { state, saveWatch, saveAiCache, today, aiReady } from '../store.js';
import { flashHint } from '../ui.js';
import { currentPage } from '../router.js';
import { inTauri } from '../bridge.js';
import { showAnalysis } from './analysis.js';
import { showKline } from './kline.js';

// 浏览器预览用的稳定假行情
function mockQuote(code) {
  const seed = [...code].reduce((a, c) => a + c.charCodeAt(0), 0);
  const price = 10 + (seed % 190) + (seed % 7) / 10;
  const pct = ((seed % 13) - 6) / 2;
  return { code, name: '模拟 ' + code.toUpperCase(), price, change: pct, change_pct: pct };
}

const pendingAi = new Set();
async function ensureAnalysis(code, q) {
  if (!aiReady()) return;
  const hit = state.aiCache[code];
  if (hit && hit.day === today()) return;
  if (pendingAi.has(code)) return;
  pendingAi.add(code);
  try {
    const res = await analyzeStock(q.name, code, q.price, q.change_pct);
    if (res && typeof res.score === 'number') {
      state.aiCache[code] = { score: res.score, analysis: res.analysis, name: q.name, day: today() };
      saveAiCache();
      if (currentPage() === 'watch') renderWatch(); // 指针归位
    }
  } catch (e) {
    console.warn('AI 分析失败:', code, e);
  } finally {
    pendingAi.delete(code);
  }
}

// 行内小仪表盘：score -100..100 → 指针 -90°..90°
function miniGauge(code, i) {
  const hit = state.aiCache[code];
  const valid = hit && hit.day === today();
  const score = valid ? hit.score : 0;
  const deg = Math.max(-100, Math.min(100, score)) * 0.9;
  const tip = !aiReady()
    ? '接入 AI 后分析（设置页填 API key）'
    : valid ? `AI 打分 ${score > 0 ? '+' : ''}${score}` : 'AI 分析中…';
  return `<svg class="w-gauge" data-i="${i}" viewBox="0 0 48 30"><title>${tip}</title>
    <path d="M7 26 A17 17 0 0 1 41 26" fill="none" stroke="#1f2430" stroke-width="4" stroke-linecap="round"/>
    <path d="M7 26 A17 17 0 0 1 41 26" fill="none" stroke="url(#ggm)" stroke-width="4" stroke-linecap="round" opacity="${valid ? 0.95 : 0.3}"/>
    <g class="needle-line" transform="rotate(${deg} 24 26)">
      <line x1="24" y1="26" x2="24" y2="11.5" stroke="${valid ? '#e8ecf2' : '#5c6470'}" stroke-width="2" stroke-linecap="round"/>
    </g>
    <circle cx="24" cy="26" r="2.4" fill="#e8ecf2"/>
  </svg>`;
}

export async function renderWatch() {
  const list = document.getElementById('watchList');
  const empty = document.getElementById('watchEmpty');
  const wl = state.watchlist;
  document.getElementById('watchMeta').textContent = wl.length ? wl.length + ' 支' : '';
  empty.style.display = wl.length ? 'none' : 'block';
  if (!wl.length) { list.innerHTML = ''; return; }

  let quotes = await fetchQuotes(wl);
  if (!quotes) quotes = wl.map(mockQuote);
  quotes.forEach((q, i) => { lastNames[wl[i]] = q.name; });

  list.innerHTML = quotes.map((q, i) => {
    const up = q.change_pct >= 0;
    return `<div class="watch-row">
      <div class="w-name"><b>${q.name}</b><i>${(wl[i] || '').toUpperCase()}</i></div>
      ${miniGauge(wl[i], i)}
      <div class="w-quote ${up ? 'up-c' : 'down-c'}">
        <b>${q.price.toFixed(2)}</b>
        <i>${up ? '+' : ''}${q.change_pct.toFixed(2)}%</i>
      </div>
      <button class="watch-del" data-i="${i}" title="删除">✕</button>
    </div>`;
  }).join('');

  // 缺当日 AI 打分的，后台逐支分析（有 key 才会真的发请求）
  quotes.forEach((q, i) => ensureAnalysis(wl[i], q));
}

function addWatch() {
  const input = document.getElementById('watchInput');
  const code = normalizeCode(input.value);
  if (!code) { flashHint('代码格式不对，例：600519 或 sh600519'); return; }
  if (state.watchlist.includes(code)) { flashHint('已在自选里了'); return; }
  state.watchlist.push(code);
  saveWatch();
  input.value = '';
  renderWatch();
}

// 点击小表盘 → AI 分析详情页
function openAnalysis(i) {
  const code = state.watchlist[i];
  if (!aiReady()) {
    flashHint(inTauri ? '先在设置页接入 AI API Key' : '浏览器预览无法调用 AI');
    return;
  }
  const hit = state.aiCache[code];
  if (!hit || hit.day !== today()) { flashHint('AI 正在分析这支股票，稍候再点'); return; }
  showAnalysis({
    title: `${hit.name} · AI 分析`,
    score: hit.score,
    text: hit.analysis,
    meta: `${code.toUpperCase()} · 分析日期 ${hit.day} · 仅供参考，不构成投资建议`,
    back: 'watch',
  });
}

let lastNames = {}; // code → 最近一次行情里的名称（K线页标题用）

export function initWatch() {
  document.getElementById('watchAddBtn').addEventListener('click', addWatch);
  document.getElementById('watchInput').addEventListener('keydown', (e) => { if (e.key === 'Enter') addWatch(); });
  document.getElementById('watchList').addEventListener('click', (e) => {
    const gauge = e.target.closest('.w-gauge');
    if (gauge) { openAnalysis(+gauge.dataset.i); return; }
    const btn = e.target.closest('.watch-del');
    if (btn) {
      state.watchlist.splice(+btn.dataset.i, 1);
      saveWatch();
      renderWatch();
      return;
    }
    // 点名称/价格区域 → K线
    const row = e.target.closest('.watch-row');
    if (row && e.target.closest('.w-name, .w-quote')) {
      const i = +row.querySelector('.w-gauge').dataset.i;
      const code = state.watchlist[i];
      showKline(code, lastNames[code]);
    }
  });
}

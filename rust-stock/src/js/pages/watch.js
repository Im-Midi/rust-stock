// pages/watch.js — 自选股：增删、行情刷新、AI 小仪表盘、K线/分析入口
// 加载策略：缓存先行（立即渲染上次行情/占位），网络刷新到达后无感重绘。
import { fetchQuotes, normalizeCode, analyzeStock, searchStocks } from '../api.js';
import { state, saveWatch, saveAiCache, storeGet, storeSet, today, aiReady } from '../store.js';
import { flashHint, nowHMS } from '../ui.js';
import { currentPage } from '../router.js';
import { inTauri } from '../bridge.js';
import { showAnalysis } from './analysis.js';
import { showKline } from './kline.js';

let cacheMap = {};      // code → 最近一次行情
let lastNames = {};     // code → 名称（K线页标题用）
let cacheLoaded = false;
let fetching = false;

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
      if (currentPage() === 'watch') paint(); // 指针归位
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
    <g class="needle-line${valid ? '' : ' dim'}" transform="rotate(${deg} 24 26)">
      <line x1="24" y1="26" x2="24" y2="11.5" stroke="${valid ? '#e8ecf2' : '#5c6470'}" stroke-width="2" stroke-linecap="round"/>
    </g>
    <circle cx="24" cy="26" r="2.4" fill="#e8ecf2"/>
  </svg>`;
}

// 只画界面，不发网络请求——永远瞬时
function paint() {
  const list = document.getElementById('watchList');
  const empty = document.getElementById('watchEmpty');
  const wl = state.watchlist;
  document.getElementById('watchMeta').textContent = wl.length ? wl.length + ' 支' : '';
  empty.style.display = wl.length ? 'none' : 'block';
  if (!wl.length) { list.innerHTML = ''; return; }

  list.innerHTML = wl.map((code, i) => {
    if (!inTauri && !cacheMap[code]) cacheMap[code] = mockQuote(code); // 预览
    const q = cacheMap[code];
    const pending = !q;
    const name = q ? q.name : (lastNames[code] || code.toUpperCase());
    const up = q ? q.change_pct >= 0 : true;
    return `<div class="watch-row">
      <div class="w-name"><b>${name}</b><i>${code.toUpperCase()}</i></div>
      ${miniGauge(code, i)}
      <div class="w-quote ${pending ? 'pending' : (up ? 'up-c' : 'down-c')}">
        <b>${pending ? '--' : q.price.toFixed(2)}</b>
        <i>${pending ? '加载中' : (up ? '+' : '') + q.change_pct.toFixed(2) + '%'}</i>
      </div>
      <button class="watch-del" data-i="${i}" title="删除">✕</button>
    </div>`;
  }).join('');
}

async function loadCacheOnce() {
  if (cacheLoaded) return;
  cacheLoaded = true;
  const saved = await storeGet('watch_quotes_cache', null);
  if (saved && typeof saved === 'object') {
    cacheMap = { ...saved, ...cacheMap };
    for (const [c, q] of Object.entries(cacheMap)) if (q && q.name) lastNames[c] = q.name;
  }
}

export async function renderWatch() {
  await loadCacheOnce();
  paint(); // 先出画面（缓存或"加载中"占位）

  if (fetching) return; // 已有请求在路上，回来会重绘
  const wl = state.watchlist;
  if (!wl.length || !inTauri) return;
  fetching = true;
  try {
    const quotes = await fetchQuotes(wl);
    if (Array.isArray(quotes)) {
      quotes.forEach(q => {
        // 按代码匹配（东财返回 6 位无前缀，新浪带前缀）
        const c = wl.find(x => x === q.code || (q.code && x.endsWith(q.code)));
        if (c) { cacheMap[c] = q; lastNames[c] = q.name; }
      });
      storeSet('watch_quotes_cache', cacheMap); // 持久化，重启后首开也秒出
      paint();
      const m = document.getElementById('watchMeta');
      if (m && state.watchlist.length) m.textContent = state.watchlist.length + ' 支 · ' + nowHMS();
      wl.forEach(c => { if (cacheMap[c]) ensureAnalysis(c, cacheMap[c]); });
    }
  } finally {
    fetching = false;
  }
}

function addByCode(code, name) {
  if (state.watchlist.includes(code)) { flashHint('已在自选里了'); return; }
  state.watchlist.push(code);
  if (name) lastNames[code] = name;
  saveWatch();
  document.getElementById('watchInput').value = '';
  hideSug();
  flashHint(`已添加：${name || code.toUpperCase()}`);
  renderWatch();
}

// ---------- 搜索建议（名称/代码/拼音首字母）----------
const mockHits = [
  { code: 'sh600519', name: '贵州茅台', market: '沪A' },
  { code: 'sz300750', name: '宁德时代', market: '深A' },
  { code: 'sz002594', name: '比亚迪', market: '深A' },
];
let sugHits = [];
let sugTimer = null;

function hideSug() {
  document.getElementById('watchSug').classList.remove('open');
  sugHits = [];
}

function showSug(hits, kw) {
  const el = document.getElementById('watchSug');
  sugHits = hits;
  if (!hits.length) {
    el.innerHTML = `<div class="sug-empty">没找到「${kw}」相关的A股</div>`;
  } else {
    el.innerHTML = hits.map((h, i) => `
      <div class="sug-item" data-i="${i}">
        <b>${h.name}</b>
        <span class="s-mkt">${h.market}</span>
        <span class="s-code">${h.code.toUpperCase()}</span>
      </div>`).join('');
  }
  el.classList.add('open');
}

async function doSearch(kw) {
  if (!inTauri) {
    showSug(mockHits.filter(h => h.name.includes(kw) || h.code.includes(kw.toLowerCase())), kw);
    return;
  }
  const hits = await searchStocks(kw);
  // 输入框内容已变化则丢弃过期结果
  if (document.getElementById('watchInput').value.trim() !== kw) return;
  showSug(hits || [], kw);
}

function onInputChange() {
  const kw = document.getElementById('watchInput').value.trim();
  clearTimeout(sugTimer);
  if (kw.length < 2 || /^(sh|sz)?\d{6}$/i.test(kw)) { hideSug(); return; } // 完整代码不必搜
  sugTimer = setTimeout(() => doSearch(kw), 280);
}

async function addWatch() {
  const input = document.getElementById('watchInput');
  const kw = input.value.trim();
  if (!kw) return;
  // 1) 标准代码直接加
  const code = normalizeCode(kw);
  if (code) { addByCode(code); return; }
  // 2) 下拉里已有结果：取第一个
  if (sugHits.length) { addByCode(sugHits[0].code, sugHits[0].name); return; }
  // 3) 现搜：唯一命中直接加，多命中弹下拉让用户选
  if (!inTauri) { flashHint('浏览器预览输入示例：茅台'); doSearch(kw); return; }
  const hits = await searchStocks(kw);
  if (!hits || !hits.length) { flashHint(`没找到「${kw}」相关的A股`); return; }
  if (hits.length === 1) { addByCode(hits[0].code, hits[0].name); return; }
  showSug(hits, kw);
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

export function initWatch() {
  document.getElementById('watchAddBtn').addEventListener('click', addWatch);
  const input = document.getElementById('watchInput');
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') addWatch();
    if (e.key === 'Escape') hideSug();
  });
  input.addEventListener('input', onInputChange);
  input.addEventListener('blur', () => setTimeout(hideSug, 180)); // 给下拉点击留时间
  document.getElementById('watchSug').addEventListener('mousedown', (e) => {
    const item = e.target.closest('.sug-item');
    if (!item) return;
    e.preventDefault(); // 防 blur 先触发
    const h = sugHits[+item.dataset.i];
    if (h) addByCode(h.code, h.name);
  });
  document.getElementById('watchList').addEventListener('click', (e) => {
    const gauge = e.target.closest('.w-gauge');
    if (gauge) { openAnalysis(+gauge.dataset.i); return; }
    const btn = e.target.closest('.watch-del');
    if (btn) {
      state.watchlist.splice(+btn.dataset.i, 1);
      saveWatch();
      paint();
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

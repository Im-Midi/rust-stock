// pages/market.js — 行情页：指数滚动条、市场情绪表盘（可翻面）、板块热力
import { INDEX_CODES, fetchQuotes, fetchSentiment, explainSentiment, fetchStockNews, classifyNews, fetchSectors } from '../api.js';
import { state, today, aiReady } from '../store.js';
import { storeGet, storeSet } from '../store.js';
import { nowHMS, flashHint } from '../ui.js';
import { inTauri, invoke } from '../bridge.js';
import { currentPage } from '../router.js';
import { isTradingDay } from '../tradingcal.js';

const mockIndices = [
  { name: '上证指数', val: '4095.45', chg: '-0.81%', up: false },
  { name: '深证成指', val: '14280.78', chg: '-0.65%', up: false },
  { name: '富时A50', val: '14820.10', chg: '+0.32%', up: true },
  { name: '台湾加权', val: '33400.32', chg: '-0.54%', up: false },
  { name: '道琼斯', val: '46558.47', chg: '-0.26%', up: false },
  { name: '纳斯达克', val: '22105.30', chg: '+0.18%', up: true },
];

// 板块热力暂为演示数据（真实板块接口在 Roadmap）
const heat = [
  { name: '机器人', chg: '+3.2%', v: 0.9 },
  { name: '人工智能', chg: '+2.7%', v: 0.78 },
  { name: '半导体', chg: '+1.4%', v: 0.55 },
  { name: '券商', chg: '+0.9%', v: 0.42 },
  { name: '消费', chg: '-0.6%', v: -0.35 },
  { name: '油气', chg: '-1.8%', v: -0.7 },
];

function heatColor(v) {
  v = Number(v) || 0; // 防 NaN（NaN 会让 rgba 失效=无底色）
  // 提高最小可见度：基础 0.24 + 随幅度增强，真实板块小涨跌也有明显红绿底
  const a = 0.24 + Math.min(0.5, Math.abs(v) * 0.55);
  const bd = Math.min(0.95, a + 0.25);
  if (v >= 0) {
    return { bg: `rgba(255,77,79,${a})`, fg: '#ff9a9b', border: `rgba(255,77,79,${bd})` };
  }
  return { bg: `rgba(20,200,125,${a})`, fg: '#7ee3b4', border: `rgba(20,200,125,${bd})` };
}

// ---------- A股交易时段判断 + 非交易时段红条 ----------
// 交易日 周一~周五；时段 09:30-11:30 / 13:00-15:00（不含法定节假日日历）
export function marketStatus(d = new Date()) {
  if (!isTradingDay(d)) return { open: false, kind: 'holiday' };
  const m = d.getHours() * 60 + d.getMinutes();
  if (m < 9 * 60 + 30) return { open: false, kind: 'pre' };
  if (m < 11 * 60 + 30) return { open: true, kind: 'open' };
  if (m < 13 * 60) return { open: false, kind: 'lunch' };
  if (m < 15 * 60) return { open: true, kind: 'open' };
  return { open: false, kind: 'closed' };
}
const MKT_TXT = {
  pre: '🔴 A股暂未开盘 · 资金流向 / 板块热力等沿用上一交易日数据',
  holiday: '🔴 A股休市（节假日/周末）· 资金流向 / 板块热力等沿用上一交易日数据',
  lunch: '🟠 午间休市（11:30–13:00）· 数据暂停更新',
  closed: '🔴 A股已收盘 · 当前为今日收盘数据',
};
export function updateMktBanner() {
  const el = document.getElementById('mktBanner');
  if (!el) return;
  const s = marketStatus();
  if (s.open) { el.classList.remove('show'); el.textContent = ''; }
  else { el.textContent = MKT_TXT[s.kind] || ''; el.classList.add('show'); }
}

export async function renderTicker() {
  let data = null;
  const quotes = await fetchQuotes(INDEX_CODES);
  if (quotes) {
    data = quotes.map(q => ({
      name: q.name,
      val: q.price.toFixed(2),
      chg: (q.change_pct >= 0 ? '+' : '') + q.change_pct.toFixed(2) + '%',
      up: q.change >= 0,
    }));
  }
  if (!data) data = mockIndices;
  const track = document.getElementById('tickerTrack');
  const make = () => data.map(i => `
    <span class="tk">
      <span class="name">${i.name}</span>
      <span class="val ${i.up ? 'up-c' : 'down-c'}">${i.val}</span>
      <span class="chg ${i.up ? 'up-c' : 'down-c'}">${i.chg}</span>
    </span>`).join('');
  track.innerHTML = make() + make(); // 复制一份用于无缝滚动
  updateMktBanner();
}

let lastSentiment = null;
export const getSentiment = () => lastSentiment;
const sentWhyCache = {}; // day|label|分桶 → AI 解读（会话内缓存）

// 缓存/沿用数据的诚实时间标注：当天 "HH:MM"，往日 "MM-DD HH:MM"
function fmtCacheTs(ts) {
  const d = new Date(ts || 0);
  const p2 = (n) => String(n).padStart(2, '0');
  const hm = `${p2(d.getHours())}:${p2(d.getMinutes())}`;
  return d.toDateString() === new Date().toDateString() ? hm : `${p2(d.getMonth() + 1)}-${p2(d.getDate())} ${hm}`;
}

// 情绪 last-good + SQLite 持久化（sent_cache）：
//   · 本次成功 → 内存 last-good + 60s 节流落盘
//   · 本次失败但有历史真实数据 → 沿用并标「沿用/缓存 HH:MM」（绝不造假）
//   · 冷启动由 hydrateMarketCache() 回填上次真实情绪，首屏不再是演示占位
let lastSentTs = 0;       // lastSentiment 对应的数据时刻
let sentFromDisk = false; // true = 来自冷启动回填（meta 标「缓存」）
let sentPersistTs = 0;    // 上次落盘时刻（60s 节流）

function paintSentiment(s, metaTxt) {
  const score = Math.max(-100, Math.min(100, s.score));
  document.getElementById('sentVal').textContent = (score > 0 ? '+' : '') + score.toFixed(1);
  document.getElementById('sentMeta').textContent = metaTxt;
  const tag = document.getElementById('sentTag');
  tag.textContent = s.label;
  if (score >= 25) { tag.style.background = 'rgba(255,77,79,.15)'; tag.style.color = 'var(--up)'; }
  else if (score <= -25) { tag.style.background = 'rgba(20,200,125,.15)'; tag.style.color = 'var(--down)'; }
  else { tag.style.background = 'rgba(245,166,35,.15)'; tag.style.color = 'var(--warn)'; }
  const needle = document.getElementById('needle');
  if (needle) {
    needle.style.transition = 'transform 1.1s cubic-bezier(.22,1,.36,1)';
    needle.style.transformBox = 'view-box';
    needle.setAttribute('transform', `rotate(${score * 0.9} 100 100)`);
  }
}

export async function renderSentiment() {
  const s = await fetchSentiment();
  if (s) {
    lastSentiment = s; lastSentTs = Date.now(); sentFromDisk = false;
    if (Date.now() - sentPersistTs > 60_000) {
      sentPersistTs = Date.now();
      storeSet('sent_cache', { s, ts: lastSentTs }); // 真实情绪落盘（SQLite）
    }
    paintSentiment(s, nowHMS());
    return;
  }
  if (lastSentiment) { // 本次失败 → 沿用最近真实数据（含冷启动回填），时间诚实标注
    paintSentiment(lastSentiment, (sentFromDisk ? '缓存 ' : '沿用 ') + fmtCacheTs(lastSentTs));
    return;
  }
  // 从未成功过且无缓存 → 演示占位（唯一兜底路径；meta 诚实标「演示数据」）
  const demo = { score: -45.97, label: '偏空谨慎', components: [
    { name: '上证指数', change_pct: -0.81, weight: 0.35 },
    { name: '深证成指', change_pct: -0.65, weight: 0.25 },
    { name: '创业板指', change_pct: -1.10, weight: 0.20 },
    { name: '沪深300', change_pct: -0.74, weight: 0.20 },
  ] };
  if (!inTauri) lastSentiment = demo; // 浏览器预览允许翻面看演示解读；真机不把演示数据留存为真
  paintSentiment(demo, '演示数据');
}

// 真实板块（取最强 4 + 最弱 2 凑 6 格）。失败回退演示数据并记录原因。
let sectorErr = '';
async function pickSectors() {
  sectorErr = '';
  let all;
  try {
    all = inTauri ? await invoke('fetch_sectors') : null;
  } catch (e) {
    sectorErr = String(e).replace(/^Error:\s*/, '').slice(0, 80);
    return null;
  }
  if (!Array.isArray(all) || all.length < 3) {
    if (Array.isArray(all)) sectorErr = `仅返回 ${all.length} 个板块`;
    return null;
  }
  const picked = all.length <= 6 ? all : [...all.slice(0, 4), ...all.slice(-2)];
  return picked.map(s => ({
    name: s.name,
    chg: (s.change_pct >= 0 ? '+' : '') + s.change_pct.toFixed(2) + '%',
    v: Math.max(-1, Math.min(1, s.change_pct / 4)), // 映射到色深 ±4%
  }));
}

// 最近一次成功的真实板块（last-good 缓存）：失败时沿用，避免频繁请求触发的
// rustls 偶发中断把界面打回"演示数据/请求失败"。
let lastSectors = null;
let lastSectorTs = 0;        // lastSectors 对应的数据时刻（本次成功 or 冷启动回填的落盘时刻）
let sectorsFromDisk = false; // true = 来自冷启动回填（meta 标「缓存」，且强制发起一次真实拉取）
let heatPersistTs = 0;       // 上次落盘时刻（60s 节流）
let heatBusy = false;

function paintHeat(data, metaTxt, real = true) {
  const grid = document.getElementById('heatGrid');
  const meta = document.getElementById('heatMeta');
  if (!grid) return;
  grid.innerHTML = data.map(h =>
    `<div class="heat-cell"><span class="h-name">${h.name}</span><span class="h-chg">${h.chg}</span></div>`
  ).join('');
  // 用 JS 赋值上色（CSSOM 不受 CSP 拦截，HTML 内联 style 在安卓会被 CSP 拦掉）
  const cells = grid.querySelectorAll('.heat-cell');
  data.forEach((h, i) => {
    const c = heatColor(h.v);
    const cell = cells[i];
    if (!cell) return;
    cell.style.background = c.bg;
    cell.style.border = `1px solid ${c.border}`;
    const chg = cell.querySelector('.h-chg');
    if (chg) chg.style.color = c.fg;
  });
  if (meta) { meta.textContent = metaTxt; meta.title = real ? '' : '演示数据（接口暂未成功）'; }
}

export async function renderHeat() {
  let data, real = true, metaTxt;

  const freshMs = Date.now() - lastSectorTs;
  if (lastSectors && !sectorsFromDisk && freshMs < 25000) {
    // 25s 内有成功数据：直接复用，不再发请求（降低偶发中断概率）
    data = lastSectors; metaTxt = nowHMS();
  } else if (heatBusy) {
    // 已有请求在途：保持现状
    if (!lastSectors) return;
    data = lastSectors;
    metaTxt = sectorsFromDisk ? '缓存 ' + fmtCacheTs(lastSectorTs) : nowHMS();
  } else {
    heatBusy = true;
    let got = null;
    try { got = await pickSectors(); } finally { heatBusy = false; }
    if (got) {
      lastSectors = got; lastSectorTs = Date.now(); sectorsFromDisk = false;
      if (Date.now() - heatPersistTs > 60_000) {
        heatPersistTs = Date.now();
        storeSet('heat_cache', { data: got, ts: lastSectorTs }); // 真实板块落盘（SQLite）
      }
      data = got; metaTxt = nowHMS();
    } else if (lastSectors) {
      // 本次失败但有历史真实数据（含冷启动回填）→ 沿用，不退演示、不报错
      data = lastSectors;
      metaTxt = (sectorsFromDisk ? '缓存 ' : '沿用 ') + fmtCacheTs(lastSectorTs);
    } else {
      // 从未成功过且无缓存 → 才回退演示
      data = heat; real = false;
      metaTxt = sectorErr ? '演示·' + sectorErr : '演示数据';
    }
  }

  paintHeat(data, metaTxt, real);
}

// ---------- 冷启动回填（SQLite heat_cache / sent_cache）----------
// main.js 启动时与 hydrateKlineCache 并发调用：上次真实板块/情绪立即上屏并
// 种入内存 last-good，离线/接口被掐时首屏也是真数据（meta 标「缓存 HH:MM」），
// 网络刷新成功后无感替换。演示数据从此只剩"从未成功且无缓存"一种触发路径。
export async function hydrateMarketCache() {
  if (!inTauri) return; // 浏览器预览走 mock，无需回填
  try {
    const [hc, sc] = await Promise.all([storeGet('heat_cache', null), storeGet('sent_cache', null)]);
    if (hc && Array.isArray(hc.data) && hc.data.length && !lastSectors) {
      lastSectors = hc.data; lastSectorTs = hc.ts || 0; sectorsFromDisk = true;
      paintHeat(hc.data, '缓存 ' + fmtCacheTs(lastSectorTs));
    }
    if (sc && sc.s && typeof sc.s.score === 'number' && !lastSentiment) {
      lastSentiment = sc.s; lastSentTs = sc.ts || 0; sentFromDisk = true;
      paintSentiment(sc.s, '缓存 ' + fmtCacheTs(lastSentTs));
    }
  } catch (e) { console.warn('行情缓存回填失败:', e); }
}

// 点击情绪表盘 → 翻面看"为什么是这个档位"
async function openSentWhy() {
  const s = lastSentiment;
  if (!s) return;
  const comps = s.components || [];
  document.getElementById('sentWhyTitle').textContent = `为什么是「${s.label}」`;
  document.getElementById('sentWhyIdx').innerHTML = comps.map(c => {
    const up = c.change_pct >= 0;
    return `<div class="why-row">
      <span class="n">${c.name}</span>
      <span class="c ${up ? 'up-c' : 'down-c'}">${up ? '+' : ''}${c.change_pct.toFixed(2)}%</span>
      <span class="w">×${(c.weight * 100).toFixed(0)}%</span>
    </div>`;
  }).join('');

  const wsum = comps.reduce((a, c) => a + c.weight, 0);
  const wavg = wsum ? comps.reduce((a, c) => a + c.change_pct * c.weight, 0) / wsum : 0;
  const sc = +s.score;
  const base = `算法：指数涨跌幅加权平均 ${wavg >= 0 ? '+' : ''}${wavg.toFixed(2)}%，tanh 压缩映射到 -100~100，得 ${sc > 0 ? '+' : ''}${sc.toFixed(1)} 分 → ${s.label}。`;
  const txtEl = document.getElementById('sentWhyTxt');
  document.getElementById('sentFlip').classList.add('flipped');

  if (!aiReady()) {
    txtEl.textContent = base + '\n\n接入 AI（设置页）后，这里会有结合盘面的进一步解读。';
    return;
  }
  const ck = today() + '|' + s.label + '|' + Math.round(sc / 5);
  if (sentWhyCache[ck]) {
    txtEl.textContent = base + '\n\nAI 解读：' + sentWhyCache[ck];
    return;
  }
  txtEl.textContent = base + '\n\nAI 解读中…';
  try {
    const detail = comps.map(c => `${c.name} ${c.change_pct >= 0 ? '+' : ''}${c.change_pct.toFixed(2)}%`).join('，');
    const why = await explainSentiment(sc, s.label, detail);
    sentWhyCache[ck] = why;
    txtEl.textContent = base + '\n\nAI 解读：' + why;
  } catch (e) {
    txtEl.textContent = base + '\n\nAI 解读失败：' + e;
  }
}

// ---------- 自选股信息：抓取与自选相关的最新快讯 ----------
const mockWatchNews = [
  { time: '2026-06-06 11:40:00', txt: '（预览示例）贵州茅台获机构密集调研，渠道反馈动销回暖', tag: '贵州茅台', stocks: [], url: '', sentiment: 1 },
  { time: '2026-06-03 10:36:00', txt: '（预览示例）宁德时代某海外项目延期，订单确认推迟', tag: '宁德时代', stocks: [], url: '', sentiment: -1 },
];
let watchNews = null;
// 标题 → 利好/利空判断的会话缓存（避免重复请求；持久化到 SQLite）
let newsSentiment = {};
let sentimentLoaded = false;

// "YYYY-MM-DD HH:MM:SS" → 当天显示 HH:MM，往日显示 MM-DD
function fmtNewsTime(s) {
  if (!s || s.length < 16) return s || '';
  const now = new Date();
  const localToday = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`;
  return s.slice(0, 10) === localToday ? s.slice(11, 16) : s.slice(5, 10);
}

let watchNewsHydrated = false;

export async function loadWatchNews() {
  if (!inTauri) { watchNews = mockWatchNews; return; }
  // 冷启动先回填上次成功的真实相关快讯（SQLite），网络刷新到手后无感替换
  if (!watchNewsHydrated) {
    watchNewsHydrated = true;
    const saved = await storeGet('watch_news_cache', null);
    if (Array.isArray(saved) && saved.length && !watchNews) watchNews = saved;
  }
  if (!state.watchlist.length) { watchNews = []; return; }
  const items = await fetchStockNews(state.watchlist);
  if (items !== null) {
    watchNews = items;
    storeSet('watch_news_cache', items.slice(0, 20));
  }
  await classifyWatchNews();
}

// 批量给未判断过的标题打利好/利空标签（AI），结果按标题缓存
async function classifyWatchNews() {
  if (!aiReady() || !watchNews || !watchNews.length) return;
  if (!sentimentLoaded) { newsSentiment = await storeGet('news_sentiment', {}); sentimentLoaded = true; }
  const todo = [...new Set(watchNews.map(n => n.txt).filter(txt => !(txt in newsSentiment)))].slice(0, 20);
  if (!todo.length) return;
  const labels = await classifyNews(todo);
  if (!labels) return;
  todo.forEach((txt, i) => { newsSentiment[txt] = labels[i] ?? 0; });
  storeSet('news_sentiment', newsSentiment);
  if (currentPage() === 'market') renderWatchNews(); // 标签到了重绘箭头
}

export function renderWatchNews() {
  const el = document.getElementById('feed');
  const meta = document.getElementById('watchNewsMeta');
  if (inTauri && !state.watchlist.length) {
    el.innerHTML = '<div class="rec-empty">添加自选股后，这里展示与它们相关的最新快讯</div>';
    meta.textContent = '相关快讯';
    return;
  }
  const list = (watchNews || []).slice(0, 6);
  if (!list.length) {
    el.innerHTML = '<div class="rec-empty">暂无自选股相关快讯（近 200 条 7×24 中未提及）<br/>全部快讯见底部「快讯」</div>';
    meta.textContent = '相关快讯';
    return;
  }
  meta.textContent = `相关 ${list.length} 条`;
  el.innerHTML = list.map((n, i) => {
    const s = (typeof n.sentiment === 'number') ? n.sentiment : (newsSentiment[n.txt] ?? null);
    let arrow = '';
    if (s === 1) arrow = '<span class="news-arrow up-c" title="利好">▲</span>';
    else if (s === -1) arrow = '<span class="news-arrow down-c" title="利空">▼</span>';
    else if (s === 0) arrow = '<span class="news-arrow neu" title="中性/影响不明">—</span>';
    // s===null：AI 尚未判断，暂不显示
    return `<div class="feed-item${n.url ? ' has-link' : ''}" data-i="${i}" title="${n.url ? '点击打开原文' : ''}">
      <span class="feed-time">${fmtNewsTime(n.time)}</span>
      <div class="feed-body">
        <div class="feed-txt">${arrow}${n.txt}</div>
        ${n.tag ? `<div class="feed-tags"><span class="tag neutral">${n.tag}</span></div>` : ''}
      </div>
    </div>`;
  }).join('');
}

// 点击条目用系统浏览器打开原文
function openWatchNews(i) {
  const n = (watchNews || [])[i];
  if (!n || !n.url) return;
  if (inTauri) invoke('plugin:opener|open_url', { url: n.url }).catch(e => console.warn('打开失败:', e));
  else window.open(n.url, '_blank');
}

export function initMarket() {
  document.getElementById('feed').addEventListener('click', (e) => {
    const row = e.target.closest('.feed-item');
    if (row && row.dataset.i != null) openWatchNews(+row.dataset.i);
  });
  document.getElementById('sentFront').addEventListener('click', openSentWhy);
  document.getElementById('sentBackBtn').addEventListener('click', (e) => {
    e.stopPropagation();
    document.getElementById('sentFlip').classList.remove('flipped');
  });
  if (!inTauri) console.log('[preview] 浏览器预览模式，行情/情绪走 mock');
  updateMktBanner();
  setInterval(updateMktBanner, 30000); // 每 30s 复核交易时段
}

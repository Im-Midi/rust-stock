// pages/market.js — 行情页：指数滚动条、市场情绪表盘（可翻面）、板块热力
import { INDEX_CODES, fetchQuotes, fetchSentiment, explainSentiment, fetchStockNews, classifyNews, fetchSectors } from '../api.js';
import { state, today, aiReady } from '../store.js';
import { storeGet, storeSet } from '../store.js';
import { nowHMS, flashHint } from '../ui.js';
import { inTauri, invoke } from '../bridge.js';
import { currentPage } from '../router.js';

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
}

let lastSentiment = null;
export const getSentiment = () => lastSentiment;
const sentWhyCache = {}; // day|label|分桶 → AI 解读（会话内缓存）

export async function renderSentiment() {
  let s = await fetchSentiment();
  if (!s) s = { score: -45.97, label: '偏空谨慎', components: [
    { name: '上证指数', change_pct: -0.81, weight: 0.35 },
    { name: '深证成指', change_pct: -0.65, weight: 0.25 },
    { name: '创业板指', change_pct: -1.10, weight: 0.20 },
    { name: '沪深300', change_pct: -0.74, weight: 0.20 },
  ] };
  lastSentiment = s;
  const score = Math.max(-100, Math.min(100, s.score));
  document.getElementById('sentVal').textContent = (score > 0 ? '+' : '') + score.toFixed(1);
  document.getElementById('sentMeta').textContent = nowHMS();
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
  if (!Array.isArray(all) || all.length < 6) {
    if (Array.isArray(all)) sectorErr = `仅返回 ${all.length} 个板块`;
    return null;
  }
  const top = all.slice(0, 4);
  const bottom = all.slice(-2);
  return [...top, ...bottom].map(s => ({
    name: s.name,
    chg: (s.change_pct >= 0 ? '+' : '') + s.change_pct.toFixed(2) + '%',
    v: Math.max(-1, Math.min(1, s.change_pct / 4)), // 映射到色深 ±4%
  }));
}

export async function renderHeat() {
  const grid = document.getElementById('heatGrid');
  let data = await pickSectors();
  const real = !!data;
  if (!data) data = heat; // 回退演示
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
  const meta = document.getElementById('heatMeta');
  if (meta) meta.textContent = real ? nowHMS() : (sectorErr ? '演示·' + sectorErr : '演示数据');
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

export async function loadWatchNews() {
  if (!inTauri) { watchNews = mockWatchNews; return; }
  if (!state.watchlist.length) { watchNews = []; return; }
  const items = await fetchStockNews(state.watchlist);
  if (items !== null) watchNews = items;
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
  if (inTauri) invoke('plugin:shell|open', { path: n.url }).catch(e => console.warn('打开失败:', e));
  else window.open(n.url, '_blank');
}

export function initMarket() {
  document.getElementById('feed').addEventListene
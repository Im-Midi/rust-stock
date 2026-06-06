// pages/market.js — 行情页：指数滚动条、市场情绪表盘（可翻面）、板块热力
import { INDEX_CODES, fetchQuotes, fetchSentiment, explainSentiment, fetchStockNews } from '../api.js';
import { state, today, aiReady } from '../store.js';
import { nowHMS, flashHint } from '../ui.js';
import { inTauri, invoke } from '../bridge.js';

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
  if (v >= 0) {
    const a = 0.12 + v * 0.22;
    return { bg: `rgba(255,77,79,${a})`, fg: '#ff7173', border: `rgba(255,77,79,${a + 0.1})` };
  }
  const a = 0.12 + (-v) * 0.22;
  return { bg: `rgba(20,200,125,${a})`, fg: '#3fd99a', border: `rgba(20,200,125,${a + 0.1})` };
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

export function renderHeat() {
  const grid = document.getElementById('heatGrid');
  grid.innerHTML = heat.map(h => {
    const c = heatColor(h.v);
    return `<div class="heat-cell" style="background:${c.bg};border:1px solid ${c.border}">
      <span class="h-name">${h.name}</span>
      <span class="h-chg" style="color:${c.fg}">${h.chg}</span>
    </div>`;
  }).join('');
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
  { time: '2026-06-06 11:40:00', txt: '（预览示例）贵州茅台获机构密集调研，渠道反馈动销回暖', tag: '贵州茅台', stocks: [], url: '' },
  { time: '2026-06-03 10:36:00', txt: '（预览示例）宁德时代中标海外大型储能项目', tag: '宁德时代', stocks: [], url: '' },
];
let watchNews = null;

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
  el.innerHTML = list.map((n, i) => `
    <div class="feed-item${n.url ? ' has-link' : ''}" data-i="${i}" title="${n.url ? '点击打开原文' : ''}">
      <span class="feed-time">${fmtNewsTime(n.time)}</span>
      <div class="feed-body">
        <div class="feed-txt">${n.txt}</div>
        ${n.tag ? `<div class="feed-tags"><span class="tag neutral">${n.tag}</span></div>` : ''}
      </div>
    </div>`).join('');
}

// 点击条目用系统浏览器打开原文
function openWatchNews(i) {
  const n = (watchNews || [])[i];
  if (!n || !n.url) return;
  if (inTauri) invoke('plugin:shell|open', { path: n.url }).catch(e => console.warn('打开失败:', e));
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
}

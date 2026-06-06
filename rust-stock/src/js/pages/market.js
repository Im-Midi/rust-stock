// pages/market.js — 行情页：指数滚动条、市场情绪表盘（可翻面）、板块热力
import { INDEX_CODES, fetchQuotes, fetchSentiment, explainSentiment } from '../api.js';
import { today, aiReady } from '../store.js';
import { nowHMS, flashHint } from '../ui.js';
import { inTauri } from '../bridge.js';

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

export function initMarket() {
  document.getElementById('sentFront').addEventListener('click', openSentWhy);
  document.getElementById('sentBackBtn').addEventListener('click', (e) => {
    e.stopPropagation();
    document.getElementById('sentFlip').classList.remove('flipped');
  });
  if (!inTauri) console.log('[preview] 浏览器预览模式，行情/情绪走 mock');
}

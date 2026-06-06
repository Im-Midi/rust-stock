// band.js — 右侧竖排仪表盘挂件：每支自选一个 AI 小仪表盘 + 名称 + 涨跌幅
// 数据自取：SQLite 优先，回退 localStorage（同源共享，兼容老数据）。
// 顶部把手拖动移动；点击任意一支（或双击把手）返回主窗。
import { inTauri, invoke } from './bridge.js';

const listEl = document.getElementById('list');
const MAX_SHOW = 8;

async function kv(key, fallback) {
  if (inTauri) {
    try {
      const v = await invoke('db_get', { key });
      if (v != null) return JSON.parse(v);
    } catch {}
  }
  try {
    const v = localStorage.getItem('rs_' + key);
    if (v != null) return JSON.parse(v);
  } catch {}
  return fallback;
}

const today = () => new Date().toISOString().slice(0, 10);

function gauge(score, valid) {
  const deg = Math.max(-100, Math.min(100, score)) * 0.9;
  return `<svg viewBox="0 0 48 30">
    <path d="M7 26 A17 17 0 0 1 41 26" fill="none" stroke="#1f2430" stroke-width="4" stroke-linecap="round"/>
    <path d="M7 26 A17 17 0 0 1 41 26" fill="none" stroke="url(#ggm)" stroke-width="4" stroke-linecap="round" opacity="${valid ? 0.95 : 0.3}"/>
    <g transform="rotate(${deg} 24 26)">
      <line x1="24" y1="26" x2="24" y2="11.5" stroke="${valid ? '#e8ecf2' : '#5c6470'}" stroke-width="2" stroke-linecap="round"/>
    </g>
    <circle cx="24" cy="26" r="2.4" fill="#e8ecf2"/>
  </svg>`;
}

function render(quotes, codes, aiCache) {
  if (!quotes.length) {
    listEl.innerHTML = '<div class="empty">暂无自选股<br/>点这里返回主窗添加</div>';
    resize(1);
    return;
  }
  const show = quotes.slice(0, MAX_SHOW);
  listEl.innerHTML = show.map((q, i) => {
    const hit = aiCache[codes[i]];
    const valid = hit && hit.day === today();
    const up = q.change_pct >= 0;
    const tip = valid ? `AI 打分 ${hit.score > 0 ? '+' : ''}${hit.score}` : '接入 AI 后显示打分';
    return `<div class="bs" title="${tip} · 点击返回主窗">
      ${gauge(valid ? hit.score : 0, valid)}
      <div class="bn">${q.name || codes[i].toUpperCase()}</div>
      <div class="bp ${up ? 'up' : 'down'}">${up ? '+' : ''}${q.change_pct.toFixed(2)}%</div>
    </div>`;
  }).join('') + (quotes.length > MAX_SHOW ? `<div class="more">还有 ${quotes.length - MAX_SHOW} 支…</div>` : '');
  resize(show.length, quotes.length > MAX_SHOW);
}

function resize(items, hasMore = false) {
  if (!inTauri) return;
  const h = 26 + 4 + items * 76 + (hasMore ? 18 : 0) + (items === 1 && !hasMore ? 30 : 0);
  invoke('resize_band', { height: Math.max(110, h) }).catch(() => {});
}

async function load() {
  if (!inTauri) {
    render(
      [{ name: '贵州茅台', change_pct: 1.85 }, { name: '宁德时代', change_pct: -0.92 }],
      ['sh600519', 'sz300750'],
      { sh600519: { score: 72, day: today() } },
    );
    return;
  }
  try {
    const wl = await kv('watchlist', []);
    if (!wl.length) { render([], [], {}); return; }
    const [settings, aiCache] = await Promise.all([kv('settings', {}), kv('ai_cache', {})]);
    const quotes = await invoke('fetch_quotes', { source: settings.source || 'sina', codes: wl });
    if (Array.isArray(quotes) && quotes.length) render(quotes, wl, aiCache);
  } catch (e) {
    console.warn('挂件加载失败:', e);
  }
}

// 点条目（或空状态）→ 返回主窗；双击把手也返回
listEl.addEventListener('click', () => { if (inTauri) invoke('restore_main'); });
document.querySelector('.grip').addEventListener('dblclick', () => { if (inTauri) invoke('restore_main'); });

load();
setInterval(load, 10_000);

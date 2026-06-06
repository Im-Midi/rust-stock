// rust-stock — 前端逻辑
// 同时兼容浏览器预览（mock）与 Tauri 运行时（真实窗口控制 / SQLite / DeepSeek）

// ---------- Tauri 桥接（浏览器中安全降级）----------
const inTauri = !!(window.__TAURI__ || window.__TAURI_INTERNALS__);

async function tauriInvoke(cmd, args) {
  if (!inTauri) { console.log('[mock invoke]', cmd, args || ''); return; }
  const { invoke } = window.__TAURI__.core;
  return invoke(cmd, args);
}

// ---------- 存储层：SQLite（权威）+ localStorage（同步缓存/浏览器回退）----------
async function storeGet(key, fallback) {
  if (inTauri) {
    try {
      const v = await tauriInvoke('db_get', { key });
      if (v != null) return JSON.parse(v);
    } catch (e) { console.warn('db_get 失败，回退 localStorage:', e); }
  }
  try {
    const v = localStorage.getItem('rs_' + key);
    if (v != null) return JSON.parse(v);
  } catch {}
  return fallback;
}
function storeSet(key, obj) {
  const json = JSON.stringify(obj);
  try { localStorage.setItem('rs_' + key, json); } catch {}
  if (inTauri) tauriInvoke('db_set', { key, value: json }).catch(e => console.warn('db_set 失败:', e));
}

// ---------- 设置 ----------
const DEFAULT_SETTINGS = { source: 'sina', interval: 10, key: '' };
let settings = { ...DEFAULT_SETTINGS };
function saveSettings(s) { settings = s; storeSet('settings', s); }

// ---------- 模拟数据（浏览器预览 / 接口失败回退）----------
const mockIndices = [
  { name: '上证指数', val: '4095.45', chg: '-0.81%', up: false },
  { name: '深证成指', val: '14280.78', chg: '-0.65%', up: false },
  { name: '富时A50', val: '14820.10', chg: '+0.32%', up: true },
  { name: '台湾加权', val: '33400.32', chg: '-0.54%', up: false },
  { name: '道琼斯', val: '46558.47', chg: '-0.26%', up: false },
  { name: '纳斯达克', val: '22105.30', chg: '+0.18%', up: true },
];

// 红涨绿跌：A股配色（板块热力暂为 mock，真实板块接口在待办里）
const heat = [
  { name: '机器人', chg: '+3.2%', v: 0.9 },
  { name: '人工智能', chg: '+2.7%', v: 0.78 },
  { name: '半导体', chg: '+1.4%', v: 0.55 },
  { name: '券商', chg: '+0.9%', v: 0.42 },
  { name: '消费', chg: '-0.6%', v: -0.35 },
  { name: '油气', chg: '-1.8%', v: -0.7 },
];

const mockNews = [
  { time: '11:33', txt: '日本重启的老旧核电站再出故障', tags: [['期货市场情报','neutral'],['核电','bear']] },
  { time: '11:15', txt: '伊朗媒体称哈尔克岛石油设施未被损坏', tags: [['能源','neutral'],['油气','bull']] },
  { time: '11:06', txt: '整治珠宝玉石等领域假证假票突出问题 两部门重拳出击', tags: [['监管','neutral']] },
  { time: '10:52', txt: '"十五五"规划首次明确支持培育一流投行', tags: [['政策','bull'],['券商','bull']] },
  { time: '10:41', txt: '春运数据超预期，出行链景气度回升', tags: [['交通运输','bull']] },
];
let newsData = mockNews;

// ---------- 窗口缩放：按宽度等比缩放整张 UI（设计基准宽 360）----------
const BASE_W = 360;
const shellEl = document.querySelector('.shell');
function applyScale() {
  const z = window.innerWidth / BASE_W;
  shellEl.style.width = BASE_W + 'px';
  shellEl.style.height = (window.innerHeight / z) + 'px';
  shellEl.style.transform = `scale(${z})`;
}
window.addEventListener('resize', applyScale);

// ---------- 代码工具 ----------
function toSourceCode(code, source) {
  if (source === 'eastmoney') return (code.startsWith('sh') ? '1.' : '0.') + code.slice(2);
  return code;
}
function normalizeCode(raw) {
  raw = (raw || '').trim().toLowerCase();
  if (/^(sh|sz)\d{6}$/.test(raw)) return raw;
  if (/^\d{6}$/.test(raw)) return (/^[569]/.test(raw) ? 'sh' : 'sz') + raw;
  return null;
}
function nowHMS() { return new Date().toTimeString().slice(0, 8); }

// ---------- 颜色映射（热力）----------
function heatColor(v) {
  if (v >= 0) {
    const a = 0.12 + v * 0.22;
    return { bg: `rgba(255,77,79,${a})`, fg: '#ff7173', border: `rgba(255,77,79,${a + 0.1})` };
  } else {
    const a = 0.12 + (-v) * 0.22;
    return { bg: `rgba(20,200,125,${a})`, fg: '#3fd99a', border: `rgba(20,200,125,${a + 0.1})` };
  }
}

// ---------- 行情抓取 ----------
const INDEX_CODES = {
  sina: ['sh000001', 'sz399001', 'sh000300', 'int_dji', 'int_nasdaq'],
  eastmoney: ['1.000001', '0.399001', '1.000300', '100.DJIA', '100.NDX'],
};

async function loadIndices() {
  if (!inTauri) return mockIndices;
  try {
    const source = settings.source;
    const quotes = await tauriInvoke('fetch_quotes', { source, codes: INDEX_CODES[source] });
    if (Array.isArray(quotes) && quotes.length) {
      return quotes.map(q => ({
        name: q.name,
        val: q.price.toFixed(2),
        chg: (q.change_pct >= 0 ? '+' : '') + q.change_pct.toFixed(2) + '%',
        up: q.change >= 0,
      }));
    }
  } catch (e) { console.warn('行情抓取失败，回退 mock:', e); }
  return mockIndices;
}

async function fetchQuotes(codes) {
  if (!inTauri) return null;
  try {
    const source = settings.source;
    const quotes = await tauriInvoke('fetch_quotes', {
      source,
      codes: codes.map(c => toSourceCode(c, source)),
    });
    return (Array.isArray(quotes) && quotes.length) ? quotes : null;
  } catch (e) { console.warn('自选行情抓取失败:', e); return null; }
}

// ---------- 渲染：指数滚动条 ----------
async function renderTicker() {
  const data = await loadIndices();
  const track = document.getElementById('tickerTrack');
  const make = () => data.map(i => `
    <span class="tk">
      <span class="name">${i.name}</span>
      <span class="val ${i.up ? 'up-c' : 'down-c'}">${i.val}</span>
      <span class="chg ${i.up ? 'up-c' : 'down-c'}">${i.chg}</span>
    </span>`).join('');
  track.innerHTML = make() + make();
}

// ---------- 渲染：市场情绪（真实数据：Rust 按指数加权计算）----------
let lastSentiment = null;     // 翻面页用
const sentWhyCache = {};      // day|label|分桶 → AI 解读（会话内缓存）

async function renderSentiment() {
  let s = null;
  if (inTauri) {
    try { s = await tauriInvoke('fetch_sentiment'); }
    catch (e) { console.warn('情绪计算失败，回退 mock:', e); }
  }
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

// ---------- 渲染：板块热力（mock，待真实板块接口）----------
function renderHeat() {
  const grid = document.getElementById('heatGrid');
  grid.innerHTML = heat.map(h => {
    const c = heatColor(h.v);
    return `<div class="heat-cell" style="background:${c.bg};border:1px solid ${c.border}">
      <span class="h-name">${h.name}</span>
      <span class="h-chg" style="color:${c.fg}">${h.chg}</span>
    </div>`;
  }).join('');
}

// ---------- 快讯（真实数据：东方财富 7×24，失败回退 mock）----------
async function loadNews() {
  if (!inTauri) return;
  try {
    const items = await tauriInvoke('fetch_news');
    if (Array.isArray(items) && items.length) {
      newsData = items.map(n => ({
        time: n.time,
        txt: n.txt,
        tags: n.tag ? [[n.tag, 'neutral']] : [],
      }));
    }
  } catch (e) { console.warn('快讯抓取失败，回退 mock:', e); }
}

function renderFeed(targetId = 'feed') {
  const el = document.getElementById(targetId);
  const list = targetId === 'feed' ? newsData.slice(0, 5) : newsData;
  el.innerHTML = list.map(n => `
    <div class="feed-item">
      <span class="feed-time">${n.time}</span>
      <div class="feed-body">
        <div class="feed-txt">${n.txt}</div>
        <div class="feed-tags">
          ${n.tags.map(t => `<span class="tag ${t[1]}">${t[0]}</span>`).join('')}
        </div>
      </div>
    </div>`).join('');
  if (targetId === 'feedFull') {
    document.getElementById('newsMeta').textContent = nowHMS();
  }
}

// ---------- 自选页 ----------
let watchlist = [];
function saveWatch() { storeSet('watchlist', watchlist); }

let aiCache = {};
function saveAiCache() { storeSet('ai_cache', aiCache); }
function today() { return new Date().toISOString().slice(0, 10); }
function aiReady() { return inTauri && !!settings.key; }

const pendingAi = new Set();
async function ensureAnalysis(code, q) {
  if (!aiReady()) return;
  const hit = aiCache[code];
  if (hit && hit.day === today()) return;
  if (pendingAi.has(code)) return;
  pendingAi.add(code);
  try {
    const res = await tauriInvoke('analyze_stock', {
      key: settings.key,
      name: q.name,
      code,
      price: q.price,
      changePct: q.change_pct,
    });
    if (res && typeof res.score === 'number') {
      aiCache[code] = { score: res.score, analysis: res.analysis, name: q.name, day: today() };
      saveAiCache();
      if (currentPage === 'watch') renderWatch();
    }
  } catch (e) {
    console.warn('AI 分析失败:', code, e);
  } finally {
    pendingAi.delete(code);
  }
}

function mockQuote(code) {
  const seed = [...code].reduce((a, c) => a + c.charCodeAt(0), 0);
  const price = 10 + (seed % 190) + (seed % 7) / 10;
  const pct = ((seed % 13) - 6) / 2;
  return { code, name: '模拟 ' + code.toUpperCase(), price, change: pct, change_pct: pct };
}

function miniGauge(code, i) {
  const hit = aiCache[code];
  const valid = hit && hit.day === today();
  const score = valid ? hit.score : 0;
  const deg = Math.max(-100, Math.min(100, score)) * 0.9;
  const tip = !aiReady()
    ? '接入 AI 后分析（设置页填 DeepSeek key）'
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

async function renderWatch() {
  const list = document.getElementById('watchList');
  const empty = document.getElementById('watchEmpty');
  document.getElementById('watchMeta').textContent = watchlist.length ? watchlist.length + ' 支' : '';
  empty.style.display = watchlist.length ? 'none' : 'block';
  if (!watchlist.length) { list.innerHTML = ''; return; }

  let quotes = await fetchQuotes(watchlist);
  if (!quotes) quotes = watchlist.map(mockQuote);

  list.innerHTML = quotes.map((q, i) => {
    const up = q.change_pct >= 0;
    return `<div class="watch-row">
      <div class="w-name"><b>${q.name}</b><i>${(watchlist[i] || '').toUpperCase()}</i></div>
      ${miniGauge(watchlist[i], i)}
      <div class="w-quote ${up ? 'up-c' : 'down-c'}">
        <b>${q.price.toFixed(2)}</b>
        <i>${up ? '+' : ''}${q.change_pct.toFixed(2)}%</i>
      </div>
      <button class="watch-del" data-i="${i}" title="删除">✕</button>
    </div>`;
  }).join('');

  quotes.forEach((q, i) => ensureAnalysis(watchlist[i], q));
}

function addWatch() {
  const input = document.getElementById('watchInput');
  const code = normalizeCode(input.value);
  if (!code) { flashHint('代码格式不对，例：600519 或 sh600519'); return; }
  if (watchlist.includes(code)) { flashHint('已在自选里了'); return; }
  watchlist.push(code);
  saveWatch();
  input.value = '';
  renderWatch();
}

document.getElementById('watchAddBtn').addEventListener('click', addWatch);
document.getElementById('watchInput').addEventListener('keydown', (e) => { if (e.key === 'Enter') addWatch(); });
document.getElementById('watchList').addEventListener('click', (e) => {
  const gauge = e.target.closest('.w-gauge');
  if (gauge) { openAnalysis(+gauge.dataset.i); return; }
  const btn = e.target.closest('.watch-del');
  if (!btn) return;
  watchlist.splice(+btn.dataset.i, 1);
  saveWatch();
  renderWatch();
});

// ---------- AI 分析详情页 ----------
function openAnalysis(i) {
  const code = watchlist[i];
  if (!aiReady()) {
    flashHint(inTauri ? '先在设置页接入 DeepSeek API Key' : '浏览器预览无法调用 AI');
    return;
  }
  const hit = aiCache[code];
  if (!hit || hit.day !== today()) { flashHint('AI 正在分析这支股票，稍候再点'); return; }
  document.getElementById('anaName').textContent = `${hit.name} · AI 分析`;
  const scoreEl = document.getElementById('anaScore');
  scoreEl.textContent = (hit.score > 0 ? '+' : '') + hit.score;
  scoreEl.className = 'gauge-val ' + (hit.score >= 0 ? 'ana-score-up' : 'ana-score-down');
  document.getElementById('anaTxt').textContent = hit.analysis || '（AI 未给出文字理由）';
  document.getElementById('anaMeta').textContent = `${code.toUpperCase()} · 分析日期 ${hit.day} · 仅供参考，不构成投资建议`;
  switchPage('analysis');
  const needle = document.getElementById('anaNeedle');
  needle.style.transition = 'none';
  needle.setAttribute('transform', 'rotate(0 100 100)');
  requestAnimationFrame(() => {
    needle.style.transition = 'transform 1s cubic-bezier(.22,1,.36,1)';
    needle.style.transformBox = 'view-box';
    needle.setAttribute('transform', `rotate(${hit.score * 0.9} 100 100)`);
  });
}

// ---------- 设置页 ----------
function initSettings() {
  document.getElementById('setSource').value = settings.source;
  document.getElementById('setInterval').value = settings.interval;
  document.getElementById('setKey').value = settings.key;
  document.getElementById('setSaveBtn').addEventListener('click', () => {
    const s = {
      source: document.getElementById('setSource').value,
      interval: Math.min(600, Math.max(3, +document.getElementById('setInterval').value || 10)),
      key: document.getElementById('setKey').value.trim(),
    };
    saveSettings(s);
    document.getElementById('setInterval').value = s.interval;
    restartTimer();
    renderTicker();
    renderWatch();
    flashHint('设置已保存（存入本地 SQLite）');
  });
}

// ---------- 页面切换 ----------
const PAGES = {
  market: 'page-market', news: 'page-news', watch: 'page-watch',
  settings: 'page-settings', analysis: 'page-analysis', chat: 'page-chat',
};
let currentPage = 'market';
function switchPage(name) {
  if (!PAGES[name]) return;
  currentPage = name;
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
  document.getElementById(PAGES[name]).classList.add('active');
  const navName = name === 'analysis' ? 'watch' : name;
  document.querySelectorAll('.nav-item').forEach(b => b.classList.toggle('active', b.dataset.page === navName));
  if (name === 'news') { loadNews().then(() => renderFeed('feedFull')); renderFeed('feedFull'); }
  if (name === 'watch') renderWatch();
  document.getElementById('body').scrollTop = 0;
}
document.querySelectorAll('.nav-item').forEach(b => b.addEventListener('click', () => switchPage(b.dataset.page)));
document.getElementById('anaBack').addEventListener('click', () => switchPage('watch'));
document.getElementById('chatBack').addEventListener('click', () => switchPage('market'));

// ---------- 情绪表盘翻面：为什么是这个档位 ----------
document.getElementById('sentFront').addEventListener('click', openSentWhy);
document.getElementById('sentBackBtn').addEventListener('click', (e) => {
  e.stopPropagation();
  document.getElementById('sentFlip').classList.remove('flipped');
});

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

  // 确定性解释（算法本身），AI 解读在其后异步补充
  const wsum = comps.reduce((a, c) => a + c.weight, 0);
  const wavg = wsum ? comps.reduce((a, c) => a + c.change_pct * c.weight, 0) / wsum : 0;
  const sc = +s.score;
  const base = `算法：指数涨跌幅加权平均 ${wavg >= 0 ? '+' : ''}${wavg.toFixed(2)}%，tanh 压缩映射到 -100~100，得 ${sc > 0 ? '+' : ''}${sc.toFixed(1)} 分 → ${s.label}。`;
  const txtEl = document.getElementById('sentWhyTxt');
  document.getElementById('sentFlip').classList.add('flipped');

  if (!aiReady()) {
    txtEl.textContent = base + '\n\n接入 DeepSeek（设置页）后，这里会有 AI 结合盘面的进一步解读。';
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
    const why = await tauriInvoke('explain_sentiment', { key: settings.key, score: sc, label: s.label, detail });
    sentWhyCache[ck] = why;
    txtEl.textContent = base + '\n\nAI 解读：' + why;
  } catch (e) {
    txtEl.textContent = base + '\n\nAI 解读失败：' + e;
  }
}

// ---------- 窗口控制 ----------
let pinned = true;
document.getElementById('pinBtn').addEventListener('click', async (e) => {
  pinned = !pinned;
  e.currentTarget.style.color = pinned ? 'var(--accent)' : 'var(--txt-2)';
  await tauriInvoke('set_always_on_top', { pinned });
});
document.getElementById('minBtn').addEventListener('click', () => tauriInvoke('minimize_window'));
document.getElementById('closeBtn').addEventListener('click', () => tauriInvoke('toggle_dock_edge'));

// ---------- AI 流式聊天 ----------
const aiInput = document.getElementById('aiInput');
document.getElementById('sendBtn').addEventListener('click', sendAI);
aiInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') sendAI(); });

let chatHistory = [];   // [{role, content}]，发给后端做上下文（截最近 12 条）
let aiBusy = false;
let curAiEl = null;
let curAiText = '';

function addBubble(role, text) {
  const empty = document.getElementById('chatEmpty');
  if (empty) empty.remove();
  const log = document.getElementById('chatLog');
  const el = document.createElement('div');
  el.className = 'msg ' + role;
  el.textContent = text;
  log.appendChild(el);
  document.getElementById('body').scrollTop = 1e9;
  return el;
}

async function sendAI() {
  const q = aiInput.value.trim();
  if (!q) return;
  if (!inTauri) { flashHint('浏览器预览无法调用 AI'); return; }
  if (!settings.key) { flashHint('先在设置页接入 DeepSeek API Key'); switchPage('settings'); return; }
  if (aiBusy) { flashHint('AI 正在回答，稍等'); return; }
  aiInput.value = '';
  switchPage('chat');
  addBubble('user', q);
  curAiEl = addBubble('ai', '');
  curAiEl.classList.add('typing');
  curAiText = '';
  aiBusy = true;
  chatHistory.push({ role: 'user', content: q });
  try {
    await tauriInvoke('ask_ai', { key: settings.key, question: q, history: chatHistory.slice(0, -1).slice(-12) });
  } catch (e) {
    failAi(String(e));
  }
}

function appendAiChunk(delta) {
  if (!curAiEl) return;
  curAiText += delta;
  curAiEl.textContent = curAiText;
  document.getElementById('body').scrollTop = 1e9;
}
function finishAi() {
  if (curAiEl) curAiEl.classList.remove('typing');
  if (curAiText) chatHistory.push({ role: 'assistant', content: curAiText });
  if (chatHistory.length > 24) chatHistory = chatHistory.slice(-24);
  aiBusy = false;
  curAiEl = null;
}
function failAi(msg) {
  if (curAiEl) {
    curAiEl.classList.remove('typing');
    curAiEl.classList.add('err');
    curAiEl.textContent = '出错了：' + msg;
  }
  aiBusy = false;
  curAiEl = null;
}

async function initAiEvents() {
  if (!inTauri) return;
  const { listen } = window.__TAURI__.event;
  await listen('ai-chunk', (e) => appendAiChunk(e.payload));
  await listen('ai-done', () => finishAi());
  await listen('ai-error', (e) => failAi(e.payload));
}

// ---------- 提示气泡 ----------
function flashHint(msg) {
  const el = document.createElement('div');
  el.textContent = msg;
  el.style.cssText = `position:absolute;bottom:90px;left:50%;transform:translateX(-50%);
    background:var(--surface-3);color:var(--txt);font-size:11px;padding:7px 14px;
    border-radius:18px;border:1px solid var(--line);z-index:99;
    box-shadow:0 6px 20px rgba(0,0,0,.4);animation:fadeIn .25s;white-space:nowrap`;
  document.querySelector('.shell').appendChild(el);
  setTimeout(() => { el.style.opacity = '0'; el.style.transition = 'opacity .3s'; }, 1400);
  setTimeout(() => el.remove(), 1800);
}

// ---------- 定时刷新 ----------
let timer = null;
function restartTimer() {
  clearInterval(timer);
  timer = setInterval(() => {
    renderTicker();
    renderSentiment();
    if (currentPage === 'watch') renderWatch();
  }, settings.interval * 1000);
}
// 快讯独立节奏：60s 一次，避免跟行情一起刷太频繁
setInterval(async () => {
  await loadNews();
  if (currentPage === 'news') renderFeed('feedFull');
  if (currentPage === 'market') renderFeed('feed');
}, 60_000);

// ---------- 初始化 ----------
(async function init() {
  applyScale();
  // 先从 SQLite 取数（浏览器回退 localStorage），再首屏渲染
  settings = { ...DEFAULT_SETTINGS, ...(await storeGet('settings', {})) };
  watchlist = await storeGet('watchlist', []);
  aiCache = await storeGet('ai_cache', {});
  initSettings();
  initAiEvents();
  renderHeat();
  renderTicker();
  renderSentiment();
  await loadNews();
  renderFeed('feed');
  restartTimer();
})();

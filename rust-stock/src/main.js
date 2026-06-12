// rust-stock — 前端入口（bootstrap）
// 模块划分见 ./js/：
//   bridge.js  Tauri 桥接（浏览器预览降级）
//   store.js   全局状态 + SQLite/localStorage 持久化
//   api.js     Tauri 命令封装（行情/快讯/情绪/AI）
//   ui.js      缩放、提示气泡等通用件
//   router.js  页面切换（渲染钩子在本文件注册）
//   pages/     行情 / 快讯 / 自选 / 聊天 / 设置
import { loadAll, state, saveSettings } from './js/store.js';
import { hydrateKlineCache } from './js/klinecache.js';
import { invoke, isMobile } from './js/bridge.js';
import { initScale } from './js/ui.js';
import { initNav, onShow, currentPage } from './js/router.js';
import { renderTicker, renderSentiment, renderHeat, initMarket, loadWatchNews, renderWatchNews, hydrateMarketCache } from './js/pages/market.js';
import { refreshCalendar } from './js/tradingcal.js';
import { checkAlarms } from './js/alarm.js';
import { loadNews, renderFeed, initNews } from './js/pages/news.js';
import { renderWatch, initWatch } from './js/pages/watch.js';
import { initChat } from './js/pages/chat.js';
import { initSettings } from './js/pages/settings.js';
import { initAnalysis } from './js/pages/analysis.js';
import { initKline } from './js/pages/kline.js';
import { renderRecommend, initRecommend } from './js/pages/recommend.js';


// ---------- 窗口控制 ----------
function initWindowControls() {
  let pinned = true;
  document.getElementById('pinBtn').addEventListener('click', async (e) => {
    pinned = !pinned;
    e.currentTarget.style.color = pinned ? 'var(--accent)' : 'var(--txt-2)';
    await invoke('set_always_on_top', { pinned });
  });
  // 最小化 = 缩为屏幕右缘的竖排仪表盘挂件（每支自选一个表盘，点挂件还原）
  document.getElementById('minBtn').addEventListener('click', () => invoke('show_band'));
  // 关闭按钮 = 弹确认：最小化到托盘 / 彻底退出（可记住选择）
  document.getElementById('closeBtn').addEventListener('click', onCloseClick);
  initCloseModal();
}

function doClose(action) {
  invoke(action === 'quit' ? 'quit_app' : 'hide_to_tray');
}

function onCloseClick() {
  const remembered = state.settings.closeAction;
  if (remembered === 'tray' || remembered === 'quit') { doClose(remembered); return; }
  document.getElementById('closeModal').classList.add('open');
}

function initCloseModal() {
  const modal = document.getElementById('closeModal');
  const remember = document.getElementById('closeRemember');
  const pick = (action) => {
    if (remember.checked) saveSettings({ ...state.settings, closeAction: action });
    modal.classList.remove('open');
    doClose(action);
  };
  document.getElementById('closeTray').addEventListener('click', () => pick('tray'));
  document.getElementById('closeQuit').addEventListener('click', () => pick('quit'));
  document.getElementById('closeCancel').addEventListener('click', () => modal.classList.remove('open'));
  modal.addEventListener('click', (e) => { if (e.target === modal) modal.classList.remove('open'); });
}

// ---------- 定时刷新（前台实时 + 后台保温）----------
// 设计：当前页每个 interval 都刷（和以前一样实时）；非当前页不再彻底变冷，
// 而是低频"保温"——每 3 个 interval 刷一次，且情绪/板块与自选行情错开 1 拍，
// 避免同一秒并发打东财（rustls 偶发掐线对请求突发最敏感）。
// 保温的真实开销很低：renderHeat 自带 25s last-good 节流，自选行情走
// fetchQuotes 单请求，AI 打分按日缓存命中后为 0 请求。
let timer = null;
let tick = 0;
function restartTimer() {
  clearInterval(timer);
  tick = 0;
  timer = setInterval(() => {
    tick++;
    const p = currentPage();
    renderTicker();                 // 顶部指数条所有页可见，始终刷新
    if (p === 'market') { renderSentiment(); renderHeat(); }      // 前台实时
    else if (tick % 3 === 0) { renderSentiment(); renderHeat(); } // 后台保温
    if (p === 'watch') renderWatch();                             // 前台实时
    else if (tick % 3 === 1) renderWatch();                       // 后台保温（错 1 拍）
  }, state.settings.interval * 1000);
}

// 快讯独立节奏：不再只刷当前页——两路快讯都后台保温，切页即见最新。
// 全量 7×24 每 60s；自选股相关错峰 20s 再拉（不与 7×24 同拍打接口）。
setInterval(() => {
  loadNews().then(() => renderFeed('feedFull'));
  setTimeout(() => { loadWatchNews().then(renderWatchNews); }, 20_000);
}, 60_000);

// ---------- 前台唤醒保温（安卓回前台立即补一拍）----------
// Android 把 App 切后台时会节流甚至冻结 WebView 定时器，回前台那一刻
// 界面可能停在几分钟前。监听 visibilitychange（focus 兜底）：一回前台就
// 并行刷新当前页 + 关键缓存。全部复用现有渲染器的 last-good/in-flight
// 去重/25s 节流，请求量与正常一拍相当；5s 防抖吞掉 visibilitychange 与
// focus 的双触发、快速切换连发，也不会与刚恢复的定时器拍子叠加双发。
let lastResumeWarm = Date.now(); // 启动流程本身就是一次全量预热，吞掉启动时的首个 focus
function warmOnResume() {
  if (document.visibilityState !== 'visible') return;
  const now = Date.now();
  if (now - lastResumeWarm < 5000) return;
  lastResumeWarm = now;
  // 全部 fire-and-forget 并行，不阻塞 UI；失败由各自的 last-good 兜底（绝不 mock）
  renderTicker();
  renderSentiment().catch(() => {});
  renderHeat();
  renderWatch(); // 自选行情（单请求，AI 打分按日缓存命中后 0 请求）
  if (currentPage() === 'news') {
    loadNews().then(() => renderFeed('feedFull'));
    setTimeout(() => { loadWatchNews().then(renderWatchNews); }, 3000);
  } else {
    loadWatchNews().then(renderWatchNews);
    setTimeout(() => { loadNews().then(() => renderFeed('feedFull')); }, 3000);
  }
  checkAlarms(); // 报警轮询可能在后台错过了拍子，回前台立即补查
}
document.addEventListener('visibilitychange', warmOnResume);
window.addEventListener('focus', warmOnResume);

// ---------- 主题（磨砂奶白/磨砂黑，按本机时间自动切换；设置可锁定）----------
export function applyTheme() {
  const m = (state.settings && state.settings.theme) || 'auto'; // auto|day|night
  const h = new Date().getHours();
  const day = m === 'day' ? true : (m === 'night' ? false : (h >= 6 && h < 18));
  document.body.classList.toggle('day', day);
  document.body.classList.toggle('glass', !!(state.settings && state.settings.glass));
}

// ---------- 启动 ----------
(async function init() {
  document.body.classList.toggle('mobile', isMobile);
  applyTheme();
  initScale();
  // SQLite 持久层并发回填：设置/自选/AI 缓存 + K线缓存 + 板块热力/情绪快照
  // （冷启动哪怕离线，K线/板块/情绪首屏都是上次真实数据，meta 标「缓存」）
  await Promise.all([loadAll(), hydrateKlineCache(), hydrateMarketCache()]);
  applyTheme(); // 设置载入后按 theme 偏好重判
  setInterval(applyTheme, 5 * 60 * 1000); // 每5分钟重判，自动在 6:00/18:00 切换

  // 切页 = 先画缓存（瞬时），再后台刷新——onShow 不被 await，导航永不等网络
  onShow('news', () => { renderFeed('feedFull'); loadNews().then(() => renderFeed('feedFull')); });
  onShow('watch', renderWatch);
  onShow('market', () => { renderWatchNews(); loadWatchNews().then(renderWatchNews); });

  initNav();
  initNews();
  initWindowControls();
  initMarket();
  initWatch();
  initChat();
  initAnalysis();
  initKline();
  initSettings(() => { restartTimer(); renderTicker(); renderWatch(); renderRecommend(); });

  // ---------- 启动并发预热：四路主数据并行拉取，互不阻塞、不阻塞首屏 ----------
  // 此前是串行 await（情绪→推荐→自选快讯…），一路被掐全队列等待，
  // 且自选/快讯页要等用户点进去才开始冷加载。现在全部并行 fire：
  //   ① 指数条 ② 情绪→推荐（推荐 prompt 依赖情绪结果，保持这一条内部顺序）
  //   ③ 板块热力 ④ 自选行情+AI ⑤ 全量快讯 ⑥ 自选相关快讯（错峰 3s，削请求尖峰）
  // 各路自己写缓存并渲染（隐藏页 DOM 照常更新），用户切过去就是现成数据。
  renderTicker();
  renderHeat();
  renderRecommend(); // 今日推荐缓存先行：有缓存立即渲染并预热缩略图K线
  renderSentiment().catch(() => {}).then(() => { initRecommend(); renderRecommend(); });
  renderWatch();
  loadNews().then(() => renderFeed('feedFull'));
  setTimeout(() => { loadWatchNews().then(renderWatchNews); }, 3000);

  restartTimer();
  refreshCalendar();                       // 拉最新节假日（best-effort）
  checkAlarms();                            // 立即查一次报警
  setInterval(checkAlarms, 60_000);        // 自选股涨跌报警轮询（每 60s）
})();

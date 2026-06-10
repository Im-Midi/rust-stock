// rust-stock — 前端入口（bootstrap）
// 模块划分见 ./js/：
//   bridge.js  Tauri 桥接（浏览器预览降级）
//   store.js   全局状态 + SQLite/localStorage 持久化
//   api.js     Tauri 命令封装（行情/快讯/情绪/AI）
//   ui.js      缩放、提示气泡等通用件
//   router.js  页面切换（渲染钩子在本文件注册）
//   pages/     行情 / 快讯 / 自选 / 聊天 / 设置
import { loadAll, state, saveSettings } from './js/store.js';
import { invoke, isMobile } from './js/bridge.js';
import { initScale } from './js/ui.js';
import { initNav, onShow, currentPage } from './js/router.js';
import { renderTicker, renderSentiment, renderHeat, initMarket, loadWatchNews, renderWatchNews } from './js/pages/market.js';
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

// ---------- 定时刷新 ----------
let timer = null;
function restartTimer() {
  clearInterval(timer);
  timer = setInterval(() => {
    const p = currentPage();
    renderTicker();                 // 顶部指数条所有页可见，始终刷新
    if (p === 'market') { renderSentiment(); renderHeat(); } // 行情页：情绪+板块实时
    if (p === 'watch') renderWatch();                        // 自选页：自选价格实时
  }, state.settings.interval * 1000);
}

// 快讯独立节奏：60s 一次（快讯页=全量 7×24；行情页=自选股相关）
setInterval(async () => {
  if (currentPage() === 'news') {
    await loadNews();
    renderFeed('feedFull');
  } else if (currentPage() === 'market') {
    await loadWatchNews();
    renderWatchNews();
  }
}, 60_000);

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
  await loadAll(); // 先取 SQLite（浏览器回退 localStorage），再首屏渲染
  applyTheme(); // 设置载入后按 theme 偏好重判
  setInterval(applyTheme, 5 * 60 * 1000); // 每5分钟重判，自动在 6:00/18:00 切换

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

  renderHeat();
  renderTicker();
  await renderSentiment(); // 推荐的盘面背景依赖情绪结果
  initRecommend();
  renderRecommend();
  await loadWatchNews();   // 行情页：自选股相关快讯
  renderWatchNews();
  restartTimer();
})();

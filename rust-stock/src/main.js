// rust-stock — 前端入口（bootstrap）
// 模块划分见 ./js/：
//   bridge.js  Tauri 桥接（浏览器预览降级）
//   store.js   全局状态 + SQLite/localStorage 持久化
//   api.js     Tauri 命令封装（行情/快讯/情绪/AI）
//   ui.js      缩放、提示气泡等通用件
//   router.js  页面切换（渲染钩子在本文件注册）
//   pages/     行情 / 快讯 / 自选 / 聊天 / 设置
import { loadAll, state } from './js/store.js';
import { invoke } from './js/bridge.js';
import { initScale } from './js/ui.js';
import { initNav, onShow, currentPage } from './js/router.js';
import { renderTicker, renderSentiment, renderHeat, initMarket } from './js/pages/market.js';
import { loadNews, renderFeed } from './js/pages/news.js';
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
  document.getElementById('minBtn').addEventListener('click', () => invoke('minimize_window'));
  // 关闭按钮 = 吸附收起到屏幕边缘（而非真正退出）
  document.getElementById('closeBtn').addEventListener('click', () => invoke('toggle_dock_edge'));
}

// ---------- 定时刷新 ----------
let timer = null;
function restartTimer() {
  clearInterval(timer);
  timer = setInterval(() => {
    renderTicker();
    renderSentiment();
    if (currentPage() === 'watch') renderWatch();
  }, state.settings.interval * 1000);
}

// 快讯独立节奏：60s 一次
setInterval(async () => {
  await loadNews();
  if (currentPage() === 'news') renderFeed('feedFull');
  if (currentPage() === 'market') renderFeed('feed');
}, 60_000);

// ---------- 启动 ----------
(async function init() {
  initScale();
  await loadAll(); // 先取 SQLite（浏览器回退 localStorage），再首屏渲染

  onShow('news', () => { renderFeed('feedFull'); loadNews().then(() => renderFeed('feedFull')); });
  onShow('watch', renderWatch);

  initNav();
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
  await loadNews();
  renderFeed('feed');
  restartTimer();
})();

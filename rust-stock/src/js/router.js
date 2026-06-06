// router.js — 页面切换。页面渲染钩子由 main.js 注册（onShow），避免循环依赖。
import { scrollBodyTop } from './ui.js';

const PAGES = {
  market: 'page-market', news: 'page-news', watch: 'page-watch',
  settings: 'page-settings', analysis: 'page-analysis', chat: 'page-chat',
  kline: 'page-kline',
};
const onShowHooks = {};
let current = 'market';

export const currentPage = () => current;
export function onShow(name, fn) { onShowHooks[name] = fn; }

export function switchPage(name) {
  if (!PAGES[name]) return;
  current = name;
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
  document.getElementById(PAGES[name]).classList.add('active');
  // 分析详情页归属"自选"导航高亮；聊天页归属"行情"
  const SUB = { analysis: 'watch', kline: 'watch', chat: 'market' };
  const navName = SUB[name] || name;
  document.querySelectorAll('.nav-item').forEach(b => b.classList.toggle('active', b.dataset.page === navName));
  if (onShowHooks[name]) onShowHooks[name]();
  scrollBodyTop();
}

export function initNav() {
  document.querySelectorAll('.nav-item').forEach(b => b.addEventListener('click', () => switchPage(b.dataset.page)));
}

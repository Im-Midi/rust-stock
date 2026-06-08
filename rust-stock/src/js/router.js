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

export function switchPage(name, push = true) {
  if (!PAGES[name]) return;
  const changed = name !== current;
  current = name;
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
  document.getElementById(PAGES[name]).classList.add('active');
  // 分析详情页归属"自选"导航高亮；聊天页归属"行情"
  const SUB = { analysis: 'watch', kline: 'watch', chat: 'market' };
  const navName = SUB[name] || name;
  document.querySelectorAll('.nav-item').forEach(b => b.classList.toggle('active', b.dataset.page === navName));
  // 压入浏览器历史，让安卓返回手势/返回键回到上一页而不是直接退出
  // （wry 的返回处理会调用 webView.goBack() → 触发 popstate；历史耗尽才退出 App）
  if (push && changed) history.pushState({ page: name }, '');
  if (onShowHooks[name]) onShowHooks[name]();
  scrollBodyTop();
}

// 系统返回（安卓手势/返回键）触发 popstate → 回到历史里的上一页
window.addEventListener('popstate', (e) => {
  const target = (e.state && e.state.page) || 'market';
  switchPage(target, false);
});

export function initNav() {
  document.querySelectorAll('.nav-item').forEach(b => b.addEventListener('click', () => switchPage(b.dataset.page)));
}

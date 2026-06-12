// pages/news.js — 快讯（东方财富 7×24 真实数据，失败回退 mock）
import { fetchNews } from '../api.js';
import { nowHMS } from '../ui.js';
import { invoke, inTauri } from '../bridge.js';
import { scoreNews } from '../sentiment.js';
import { storeGet, storeSet } from '../store.js';

const mockNews = [
  { time: '11:33', txt: '日本重启的老旧核电站再出故障', url: '', tags: [['期货市场情报','neutral'],['核电','bear']] },
  { time: '11:15', txt: '伊朗媒体称哈尔克岛石油设施未被损坏', url: '', tags: [['能源','neutral'],['油气','bull']] },
  { time: '11:06', txt: '整治珠宝玉石等领域假证假票突出问题 两部门重拳出击', url: '', tags: [['监管','neutral']] },
  { time: '10:52', txt: '"十五五"规划首次明确支持培育一流投行', url: '', tags: [['政策','bull'],['券商','bull']] },
  { time: '10:41', txt: '春运数据超预期，出行链景气度回升', url: '', tags: [['交通运输','bull']] },
];
// 真机不渲染 mock（no-mock-on-device）：未加载到真实数据前显示"加载中"占位，
// 冷启动先回填上次成功的真实快讯（SQLite），到手后台刷新无感替换。
let newsData = inTauri ? [] : mockNews;
let newsHydrated = false;

async function hydrateNewsCache() {
  if (newsHydrated || !inTauri) return;
  newsHydrated = true;
  const saved = await storeGet('news_cache', null);
  if (Array.isArray(saved) && saved.length && !newsData.length) newsData = saved;
}

export async function loadNews() {
  await hydrateNewsCache();
  const items = await fetchNews();
  if (items) {
    newsData = items.map(n => ({
      time: n.time,
      txt: n.txt,
      url: n.url || '',
      tags: n.tag ? [[n.tag, 'bull']] : [],
    }));
    if (inTauri) storeSet('news_cache', newsData.slice(0, 40)); // 冷启动秒出
  }
}

export function renderFeed(targetId = 'feed') {
  const el = document.getElementById(targetId);
  if (!newsData.length) {
    el.innerHTML = '<div class="rec-empty">快讯加载中…</div>';
    if (targetId === 'feedFull') document.getElementById('newsMeta').textContent = '';
    return;
  }
  const list = targetId === 'feed' ? newsData.slice(0, 5) : newsData;
  el.innerHTML = list.map((n, i) => {
    const sen = scoreNews(n.txt); // 本地词典打分，零成本不耗 AI token
    const senTag = sen.label === 'bull' ? '<span class="tag bull" title="本地词典：利好">利好</span>'
      : sen.label === 'bear' ? '<span class="tag bear" title="本地词典：利空">利空</span>' : '';
    return `
    <div class="feed-item${n.url ? ' has-link' : ''}" data-i="${i}" title="${n.url ? '点击看原文' : ''}">
      <span class="feed-time">${n.time}</span>
      <div class="feed-body">
        <div class="feed-txt">${n.txt}</div>
        <div class="feed-tags">
          ${senTag}${n.tags.map(t => `<span class="tag ${t[1]}">${t[0]}</span>`).join('')}
        </div>
      </div>
    </div>`;
  }).join('');
  if (targetId === 'feedFull') {
    document.getElementById('newsMeta').textContent = nowHMS();
  }
}

// 点击快讯条目用系统浏览器打开东财原文
function openNews(i) {
  const n = newsData[i];
  if (!n || !n.url) return;
  if (inTauri) invoke('plugin:opener|open_url', { url: n.url }).catch(e => console.warn('打开失败:', e));
  else window.open(n.url, '_blank');
}

export function initNews() {
  const el = document.getElementById('feedFull');
  if (el) el.addEventListener('click', (e) => {
    const row = e.target.closest('.feed-item');
    if (row && row.dataset.i != null) openNews(+row.dataset.i);
  });
}

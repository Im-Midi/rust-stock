// pages/news.js — 快讯（东方财富 7×24 真实数据，失败回退 mock）
import { fetchNews } from '../api.js';
import { nowHMS } from '../ui.js';

const mockNews = [
  { time: '11:33', txt: '日本重启的老旧核电站再出故障', tags: [['期货市场情报','neutral'],['核电','bear']] },
  { time: '11:15', txt: '伊朗媒体称哈尔克岛石油设施未被损坏', tags: [['能源','neutral'],['油气','bull']] },
  { time: '11:06', txt: '整治珠宝玉石等领域假证假票突出问题 两部门重拳出击', tags: [['监管','neutral']] },
  { time: '10:52', txt: '"十五五"规划首次明确支持培育一流投行', tags: [['政策','bull'],['券商','bull']] },
  { time: '10:41', txt: '春运数据超预期，出行链景气度回升', tags: [['交通运输','bull']] },
];
let newsData = mockNews;

export async function loadNews() {
  const items = await fetchNews();
  if (items) {
    newsData = items.map(n => ({
      time: n.time,
      txt: n.txt,
      tags: n.tag ? [[n.tag, 'bull']] : [],
    }));
  }
}

export function renderFeed(targetId = 'feed') {
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

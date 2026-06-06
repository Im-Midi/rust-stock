// pages/analysis.js — AI 分析详情页（自选股打分 / 今日推荐共用）
import { switchPage } from '../router.js';

let backTo = 'watch';

export function initAnalysis() {
  document.getElementById('anaBack').addEventListener('click', () => switchPage(backTo));
}

/// 展示一份带打分的分析。{ title, score, text, meta, back }
export function showAnalysis({ title, score, text, meta, back }) {
  backTo = back || 'watch';
  document.getElementById('anaName').textContent = title;
  const scoreEl = document.getElementById('anaScore');
  scoreEl.textContent = (score > 0 ? '+' : '') + score;
  scoreEl.className = 'gauge-val ' + (score >= 0 ? 'ana-score-up' : 'ana-score-down');
  document.getElementById('anaTxt').textContent = text || '（AI 未给出文字理由）';
  document.getElementById('anaMeta').textContent = meta || '';
  switchPage('analysis');
  // 指针从 0 摆到分数位
  const needle = document.getElementById('anaNeedle');
  needle.style.transition = 'none';
  needle.setAttribute('transform', 'rotate(0 100 100)');
  requestAnimationFrame(() => {
    needle.style.transition = 'transform 1s cubic-bezier(.22,1,.36,1)';
    needle.style.transformBox = 'view-box';
    needle.setAttribute('transform', `rotate(${score * 0.9} 100 100)`);
  });
}

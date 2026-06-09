// pages/chat.js — AI 流式聊天 + 深度调研收藏（整段文字按分组永久存本机）
import { askAi } from '../api.js';
import { state, saveResearch } from '../store.js';
import { inTauri, listen } from '../bridge.js';
import { flashHint, scrollBodyBottom } from '../ui.js';
import { switchPage } from '../router.js';

let chatHistory = []; // [{role, content}]，发请求时截最近 12 条做上下文
let aiBusy = false;
let curAiEl = null;
let curAiText = '';
let curIsResearch = false;
let curTopic = '';
let lastResearch = null; // 最近一次深度调研结果 { topic, text }

const ts = () => { const d = new Date(); const p = n => String(n).padStart(2, '0'); return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`; };
const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function addBubble(role, text) {
  const empty = document.getElementById('chatEmpty');
  if (empty) empty.remove();
  const log = document.getElementById('chatLog');
  const el = document.createElement('div');
  el.className = 'msg ' + role;
  el.textContent = text;
  log.appendChild(el);
  scrollBodyBottom();
  return el;
}

async function sendAI() {
  const input = document.getElementById('aiInput');
  const q = input.value.trim();
  if (!q) return;
  if (!inTauri) { flashHint('浏览器预览无法调用 AI'); return; }
  if (!state.settings.key) { flashHint('先在设置页接入 AI API Key'); switchPage('settings'); return; }
  if (aiBusy) { flashHint('AI 正在回答，稍等'); return; }
  input.value = '';
  switchPage('chat');
  addBubble('user', q);
  curAiEl = addBubble('ai', '');
  curAiEl.classList.add('typing');
  curAiText = '';
  aiBusy = true;
  chatHistory.push({ role: 'user', content: q });
  const mode = document.getElementById('deepBtn').classList.contains('on') ? 'research' : null;
  curIsResearch = mode === 'research';
  curTopic = q;
  document.getElementById('saveBar').classList.remove('show'); // 新提问先收起保存条
  try {
    await askAi(q, chatHistory.slice(0, -1).slice(-12), mode);
  } catch (e) {
    failAi(String(e));
  }
}

function appendAiChunk(delta) {
  if (!curAiEl) return;
  curAiText += delta;
  curAiEl.textContent = curAiText;
  scrollBodyBottom();
}

function finishAi() {
  if (curAiEl) curAiEl.classList.remove('typing');
  if (curAiText) chatHistory.push({ role: 'assistant', content: curAiText });
  if (chatHistory.length > 24) chatHistory = chatHistory.slice(-24);
  // 深度调研完成 → 记下整段结果，输入框上方浮出「加入分组」
  if (curIsResearch && curAiText && curAiText.length > 40) {
    lastResearch = { topic: curTopic, text: curAiText };
    document.getElementById('saveBar').classList.add('show');
  }
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

// ---------- 加入分组 ----------
function openGroupPick() {
  if (!lastResearch) { flashHint('先做一次深度调研'); return; }
  document.getElementById('groupPick').innerHTML = state.research.groups.map(g => {
    const cnt = state.research.saved.filter(s => s.groupId === g.id).length;
    return `<button class="m-btn group-pick-btn" data-g="${g.id}">${esc(g.name)}（${cnt}）</button>`;
  }).join('');
  document.getElementById('groupModal').classList.add('open');
}
function saveToGroup(gid) {
  if (!lastResearch) return;
  state.research.saved.push({ id: 'r' + Date.now(), groupId: gid, topic: lastResearch.topic, text: lastResearch.text, time: ts() });
  saveResearch();
  document.getElementById('groupModal').classList.remove('open');
  document.getElementById('saveBar').classList.remove('show');
  flashHint('已加入分组');
}

// ---------- 收藏浏览 / 改名 / 删除 ----------
function renderFav() {
  document.getElementById('favBody').innerHTML = state.research.groups.map(g => {
    const items = state.research.saved.filter(s => s.groupId === g.id);
    const ih = items.length ? items.map(it => `
      <div class="fav-item" data-id="${it.id}">
        <div class="fi-head"><span>${it.time} · ${esc(it.topic || '调研')}</span><i class="fi-del" data-del="${it.id}">删除</i></div>
        <div class="fi-text" data-exp="${it.id}">${esc(it.text)}</div>
      </div>`).join('') : '<div class="fav-empty">（空，做完深度调研点「加入分组」存这里）</div>';
    return `<div class="fav-group"><h4><span data-gname="${g.id}">${esc(g.name)}</span><button class="ren" data-ren="${g.id}">改名</button></h4>${ih}</div>`;
  }).join('') + '<div class="fav-empty" style="margin-top:6px">同步到云端（多设备）为付费功能，敬请期待。</div>';
}
function openFav() { renderFav(); document.getElementById('favModal').classList.add('open'); }

function startRename(gid) {
  const g = state.research.groups.find(x => x.id === gid);
  const span = document.querySelector(`[data-gname="${gid}"]`);
  if (!g || !span) return;
  const inp = document.createElement('input');
  inp.value = g.name;
  inp.className = 'w-input';
  inp.style.cssText = 'width:130px;height:26px;font-size:12px';
  inp.maxLength = 8;
  span.replaceWith(inp);
  inp.focus(); inp.select();
  const commit = () => { const v = inp.value.trim(); if (v) { g.name = v; saveResearch(); } renderFav(); };
  inp.addEventListener('blur', commit);
  inp.addEventListener('keydown', (e) => { if (e.key === 'Enter') inp.blur(); });
}

export function initChat() {
  const aiInput = document.getElementById('aiInput');
  document.getElementById('sendBtn').addEventListener('click', sendAI);
  aiInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') sendAI(); });
  document.getElementById('chatBack').addEventListener('click', () => switchPage('market'));
  const deepBtn = document.getElementById('deepBtn');
  deepBtn.addEventListener('click', () => {
    const on = deepBtn.classList.toggle('on');
    aiInput.placeholder = on ? '深度调研：输入主题，如 AI算力光模块…' : '问问 AI 助手…';
    if (on) flashHint('深度调研已开启：输入主题做产业链八层拆解');
  });
  // 收藏功能
  document.getElementById('saveResBtn').addEventListener('click', openGroupPick);
  document.getElementById('groupCancel').addEventListener('click', () => document.getElementById('groupModal').classList.remove('open'));
  document.getElementById('groupModal').addEventListener('click', (e) => { if (e.target.id === 'groupModal') e.currentTarget.classList.remove('open'); });
  document.getElementById('groupPick').addEventListener('click', (e) => { const b = e.target.closest('[data-g]'); if (b) saveToGroup(b.dataset.g); });
  document.getElementById('favBtn').addEventListener('click', openFav);
  document.getElementById('favClose').addEventListener('click', () => document.getElementById('favModal').classList.remove('open'));
  document.getElementById('favModal').addEventListener('click', (e) => { if (e.target.id === 'favModal') e.currentTarget.classList.remove('open'); });
  document.getElementById('favBody').addEventListener('click', (e) => {
    const del = e.target.closest('[data-del]');
    if (del) { state.research.saved = state.research.saved.filter(s => s.id !== del.dataset.del); saveResearch(); renderFav(); return; }
    const ren = e.target.closest('[data-ren]');
    if (ren) { startRename(ren.dataset.ren); return; }
    const exp = e.target.closest('[data-exp]');
    if (exp) { exp.parentElement.classList.toggle('expanded'); return; }
  });
  listen('ai-chunk', (e) => appendAiChunk(e.payload));
  listen('ai-done', () => finishAi());
  listen('ai-error', (e) => failAi(e.payload));
}

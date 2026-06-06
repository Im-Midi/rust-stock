// pages/chat.js — AI 流式聊天（事件：ai-chunk / ai-done / ai-error）
import { askAi } from '../api.js';
import { state } from '../store.js';
import { inTauri, listen } from '../bridge.js';
import { flashHint, scrollBodyBottom } from '../ui.js';
import { switchPage } from '../router.js';

let chatHistory = []; // [{role, content}]，发请求时截最近 12 条做上下文
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
  try {
    await askAi(q, chatHistory.slice(0, -1).slice(-12));
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

export function initChat() {
  const aiInput = document.getElementById('aiInput');
  document.getElementById('sendBtn').addEventListener('click', sendAI);
  aiInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') sendAI(); });
  document.getElementById('chatBack').addEventListener('click', () => switchPage('market'));
  listen('ai-chunk', (e) => appendAiChunk(e.payload));
  listen('ai-done', () => finishAi());
  listen('ai-error', (e) => failAi(e.payload));
}

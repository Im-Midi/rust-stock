// pages/settings.js — 设置：数据源、刷新间隔、主题、AI Provider、调研收藏
import { state, saveSettings } from '../store.js';
import { listSources } from '../api.js';
import { flashHint } from '../ui.js';
import { inTauri, invoke } from '../bridge.js';
import { openFav } from './chat.js';

// 开机自启动（tauri-plugin-autostart）
async function initAutostart() {
  const btn = document.getElementById('autostartBtn');
  if (!btn) return;
  if (!inTauri) { btn.textContent = '浏览器预览不可用'; return; }
  const render = (on) => {
    btn.textContent = on ? '已开启（点击关闭）' : '已关闭（点击开启）';
    btn.style.background = on ? 'var(--accent)' : 'var(--surface-3)';
    btn.style.color = on ? '#fff' : 'var(--txt-2)';
  };
  let on = false;
  try { on = !!(await invoke('plugin:autostart|is_enabled')); }
  catch (e) { btn.textContent = '检测失败'; console.warn(e); return; }
  render(on);
  btn.addEventListener('click', async () => {
    try {
      await invoke(on ? 'plugin:autostart|disable' : 'plugin:autostart|enable');
      on = !on; render(on);
      flashHint(on ? '已设为开机自启' : '已取消开机自启');
    } catch (e) { flashHint('设置失败：' + e); }
  });
}

// 本地按主题偏好立即套用（与 main.js applyTheme 逻辑一致，避免循环依赖）
function applyThemeNow() {
  const m = state.settings.theme || 'auto';
  const h = new Date().getHours();
  const day = m === 'day' ? true : (m === 'night' ? false : (h >= 6 && h < 18));
  document.body.classList.toggle('day', day);
  document.body.classList.toggle('glass', !!state.settings.glass);
}

export async function initSettings(onSaved) {
  initAutostart();

  const sel = document.getElementById('setSource');
  const sources = await listSources();
  if (Array.isArray(sources) && sources.length) {
    sel.innerHTML = sources.map(s => `<option value="${s.id}">${s.name}</option>`).join('');
  }
  sel.value = state.settings.source;
  document.getElementById('setInterval').value = state.settings.interval;
  document.getElementById('setKey').value = state.settings.key;
  document.getElementById('setAiBase').value = state.settings.aiBase;
  document.getElementById('setAiModel').value = state.settings.aiModel;

  // ---- 主题分段控件 ----
  const renderTheme = () => document.querySelectorAll('#themeSeg button')
    .forEach(b => b.classList.toggle('on', b.dataset.theme === (state.settings.theme || 'auto')));
  renderTheme();
  document.getElementById('themeSeg').addEventListener('click', (e) => {
    const b = e.target.closest('[data-theme]');
    if (!b) return;
    saveSettings({ ...state.settings, theme: b.dataset.theme });
    applyThemeNow();
    renderTheme();
  });

  // ---- 液态玻璃外观开关 ----
  const renderGlass = () => document.querySelectorAll('#glassSeg button')
    .forEach(b => b.classList.toggle('on', b.dataset.glass === (state.settings.glass ? 'on' : 'off')));
  renderGlass();
  document.getElementById('glassSeg').addEventListener('click', (e) => {
    const b = e.target.closest('[data-glass]');
    if (!b) return;
    saveSettings({ ...state.settings, glass: b.dataset.glass === 'on' });
    applyThemeNow();
    renderGlass();
  });

  // ---- API Key 折叠 / 一键清除 ----
  const keySet = document.getElementById('keySet');
  const keyEditBox = document.getElementById('keyEditBox');
  const showKeyState = () => {
    const has = !!state.settings.key;
    keySet.style.display = has ? 'block' : 'none';
    keyEditBox.style.display = has ? 'none' : 'block';
  };
  showKeyState();
  document.getElementById('keyEdit').addEventListener('click', () => {
    keySet.style.display = 'none'; keyEditBox.style.display = 'block';
    document.getElementById('setKey').focus();
  });
  document.getElementById('keyClear').addEventListener('click', () => {
    saveSettings({ ...state.settings, key: '' });
    document.getElementById('setKey').value = '';
    showKeyState();
    flashHint('已清除 API Key');
    if (onSaved) onSaved();
  });
  document.getElementById('keyX').addEventListener('click', () => {
    const k = document.getElementById('setKey'); k.value = ''; k.focus();
  });

  // ---- 我的调研收藏 ----
  document.getElementById('openFavBtn').addEventListener('click', openFav);

  // 关闭行为重置（桌面）
  const rc = document.getElementById('resetCloseBtn');
  if (rc) rc.addEventListener('click', () => {
    saveSettings({ ...state.settings, closeAction: '' });
    flashHint('已重置：下次关闭会重新询问');
  });

  document.getElementById('setSaveBtn').addEventListener('click', () => {
    const s = {
      ...state.settings,
      source: sel.value,
      interval: Math.min(600, Math.max(3, +document.getElementById('setInterval').value || 10)),
      key: document.getElementById('setKey').value.trim(),
      aiBase: document.getElementById('setAiBase').value.trim(),
      aiModel: document.getElementById('setAiModel').value.trim(),
    };
    saveSettings(s);
    document.getElementById('setInterval').value = s.interval;
    showKeyState();
    flashHint('设置已保存（存入本地 SQLite）');
    if (onSaved) onSaved();
  });
}

// pages/settings.js — 设置：数据源（注册表动态生成）、刷新间隔、AI Provider
import { state, saveSettings } from '../store.js';
import { listSources } from '../api.js';
import { flashHint } from '../ui.js';
import { inTauri, invoke } from '../bridge.js';

// 开机自启动（tauri-plugin-autostart）
async function initAutostart() {
  const btn = document.getElementById('autostartBtn');
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
      on = !on;
      render(on);
      flashHint(on ? '已设为开机自启' : '已取消开机自启');
    } catch (e) { flashHint('设置失败：' + e); }
  });
}

export async function initSettings(onSaved) {
  initAutostart();
  const sel = document.getElementById('setSource');
  // 数据源下拉框由 Rust 注册表动态生成；浏览器预览保留 HTML 里的静态项
  const sources = await listSources();
  if (Array.isArray(sources) && sources.length) {
    sel.innerHTML = sources.map(s => `<option value="${s.id}">${s.name}</option>`).join('');
  }
  sel.value = state.settings.source;
  document.getElementById('setInterval').value = state.settings.interval;
  document.getElementById('setKey').value = state.settings.key;
  document.getElementById('setAiBase').value = state.settings.aiBase;
  document.getElementById('setAiModel').value = state.settings.aiModel;

  // 重置"关闭行为"记忆（下次关闭重新询问）
  const rc = document.getElementById('resetCloseBtn');
  if (rc) rc.addEventListener('click', () => {
    saveSettings({ ...state.settings, closeAction: '' });
    flashHint('已重置：下次关闭会重新询问');
  });

  document.getElementById('setSaveBtn').addEventListener('click', () => {
    const s = {
      source: sel.value,
      interval: Math.min(600, Math.max(3, +document.getElementById('setInterval').value || 10)),
      key: document.getElementById('setKey').value.trim(),
      aiBase: document.getElementById('setAiBase').value.trim(),
      aiModel: document.getElementById('setAiModel').value.trim(),
    };
    saveSettings(s);
    document.getElementById('setInterval').value = s.interval;
    flashHint('设置已保存（存入本地 SQLite）');
    if (onSaved) onSaved();
  });
}

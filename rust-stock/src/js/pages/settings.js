// pages/settings.js — 设置：数据源（注册表动态生成）、刷新间隔、AI Provider
import { state, saveSettings } from '../store.js';
import { listSources } from '../api.js';
import { flashHint } from '../ui.js';

export async function initSettings(onSaved) {
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

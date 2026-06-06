// bridge.js — Tauri 桥接层。浏览器预览时安全降级（mock）。
export const inTauri = !!(window.__TAURI__ || window.__TAURI_INTERNALS__);

export async function invoke(cmd, args) {
  if (!inTauri) { console.log('[mock invoke]', cmd, args || ''); return; }
  return window.__TAURI__.core.invoke(cmd, args);
}

export async function listen(event, cb) {
  if (!inTauri) return;
  return window.__TAURI__.event.listen(event, cb);
}

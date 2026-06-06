// ui.js — 通用 UI 工具：等比缩放、提示气泡、时间
const BASE_W = 360;
const shellEl = document.querySelector('.shell');

export function applyScale() {
  const z = window.innerWidth / BASE_W;
  shellEl.style.width = BASE_W + 'px';
  shellEl.style.height = (window.innerHeight / z) + 'px';
  shellEl.style.transform = `scale(${z})`;
}
export function initScale() {
  applyScale();
  window.addEventListener('resize', applyScale);
}

export function flashHint(msg) {
  const el = document.createElement('div');
  el.textContent = msg;
  el.style.cssText = `position:absolute;bottom:90px;left:50%;transform:translateX(-50%);
    background:var(--surface-3);color:var(--txt);font-size:11px;padding:7px 14px;
    border-radius:18px;border:1px solid var(--line);z-index:99;
    box-shadow:0 6px 20px rgba(0,0,0,.4);animation:fadeIn .25s;white-space:nowrap`;
  document.querySelector('.shell').appendChild(el);
  setTimeout(() => { el.style.opacity = '0'; el.style.transition = 'opacity .3s'; }, 1400);
  setTimeout(() => el.remove(), 1800);
}

export const nowHMS = () => new Date().toTimeString().slice(0, 8);
export const scrollBodyTop = () => { document.getElementById('body').scrollTop = 0; };
export const scrollBodyBottom = () => { document.getElementById('body').scrollTop = 1e9; };

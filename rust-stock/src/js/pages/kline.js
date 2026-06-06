// pages/kline.js — K线图（canvas 蜡烛图 + MA5/MA10 + 成交量），自选股点名称进入
import { fetchKline } from '../api.js';
import { switchPage } from '../router.js';
import { inTauri } from '../bridge.js';

const cur = { code: null, name: '', period: 'day' };

// 浏览器预览：按代码种子生成稳定的随机游走K线
function mockCandles(code, n = 90) {
  let seed = [...code].reduce((a, c) => a + c.charCodeAt(0), 7);
  const rand = () => { seed = (seed * 9301 + 49297) % 233280; return seed / 233280; };
  let price = 20 + (seed % 180);
  const out = [];
  const d = new Date();
  d.setDate(d.getDate() - n);
  for (let i = 0; i < n; i++) {
    d.setDate(d.getDate() + 1);
    const open = price;
    const drift = (rand() - 0.48) * price * 0.04;
    const close = Math.max(1, open + drift);
    const high = Math.max(open, close) * (1 + rand() * 0.015);
    const low = Math.min(open, close) * (1 - rand() * 0.015);
    out.push({
      date: d.toISOString().slice(0, 10),
      open, close, high, low,
      volume: 10000 + rand() * 90000,
    });
    price = close;
  }
  return out;
}

function ma(candles, n) {
  return candles.map((_, i) => {
    if (i < n - 1) return null;
    let s = 0;
    for (let j = i - n + 1; j <= i; j++) s += candles[j].close;
    return s / n;
  });
}

function draw(candles) {
  const cv = document.getElementById('klineCanvas');
  const ctx = cv.getContext('2d');
  const W = cv.width, H = cv.height;
  ctx.clearRect(0, 0, W, H);

  const padL = 8, padR = 64, padT = 14;
  const priceH = H * 0.68;            // 上 68% 画蜡烛
  const volTop = priceH + 24;         // 下方画成交量
  const volH = H - volTop - 10;
  const plotW = W - padL - padR;

  const hi = Math.max(...candles.map(c => c.high));
  const lo = Math.min(...candles.map(c => c.low));
  const span = hi - lo || 1;
  const y = p => padT + (hi - p) / span * (priceH - padT);
  const maxVol = Math.max(...candles.map(c => c.volume)) || 1;

  const n = candles.length;
  const step = plotW / n;
  const bw = Math.max(2, step * 0.62);

  const UP = '#ff4d4f', DOWN = '#14c87d';

  // 网格 + 右侧价格刻度
  ctx.font = '16px "DM Mono", monospace';
  ctx.textBaseline = 'middle';
  for (let i = 0; i <= 4; i++) {
    const p = hi - span * i / 4;
    const yy = y(p);
    ctx.strokeStyle = 'rgba(255,255,255,.05)';
    ctx.beginPath(); ctx.moveTo(padL, yy); ctx.lineTo(W - padR, yy); ctx.stroke();
    ctx.fillStyle = '#5c6470';
    ctx.fillText(p.toFixed(2), W - padR + 6, yy);
  }

  // 蜡烛 + 成交量（A股红涨绿跌）
  candles.forEach((c, i) => {
    const x = padL + i * step + step / 2;
    const up = c.close >= c.open;
    ctx.strokeStyle = ctx.fillStyle = up ? UP : DOWN;
    // 影线
    ctx.beginPath(); ctx.moveTo(x, y(c.high)); ctx.lineTo(x, y(c.low)); ctx.stroke();
    // 实体
    const yo = y(c.open), yc = y(c.close);
    const top = Math.min(yo, yc), hgt = Math.max(1.5, Math.abs(yo - yc));
    if (up) { ctx.fillStyle = '#12151c'; ctx.fillRect(x - bw / 2, top, bw, hgt); ctx.strokeRect(x - bw / 2, top, bw, hgt); }
    else ctx.fillRect(x - bw / 2, top, bw, hgt);
    // 成交量
    const vh = c.volume / maxVol * volH;
    ctx.fillStyle = up ? 'rgba(255,77,79,.5)' : 'rgba(20,200,125,.5)';
    ctx.fillRect(x - bw / 2, H - 10 - vh, bw, vh);
  });

  // MA5 / MA10
  const drawMa = (data, color) => {
    ctx.strokeStyle = color; ctx.lineWidth = 1.6;
    ctx.beginPath();
    let started = false;
    data.forEach((v, i) => {
      if (v == null) return;
      const x = padL + i * step + step / 2;
      if (!started) { ctx.moveTo(x, y(v)); started = true; }
      else ctx.lineTo(x, y(v));
    });
    ctx.stroke(); ctx.lineWidth = 1;
  };
  drawMa(ma(candles, 5), '#f5a623');
  drawMa(ma(candles, 10), '#4d8dff');

  // 首尾日期
  ctx.fillStyle = '#5c6470';
  ctx.textBaseline = 'alphabetic';
  ctx.fillText(candles[0].date, padL, priceH + 18);
  const lastD = candles[n - 1].date;
  ctx.fillText(lastD, W - padR - ctx.measureText(lastD).width, priceH + 18);

  // 底部信息
  const last = candles[n - 1];
  const chg = (last.close - last.open) / last.open * 100;
  document.getElementById('klineInfo').innerHTML = `
    <span>开 <b>${last.open.toFixed(2)}</b></span>
    <span>收 <b class="${last.close >= last.open ? 'up-c' : 'down-c'}">${last.close.toFixed(2)}</b></span>
    <span>高 <b>${last.high.toFixed(2)}</b></span>
    <span>低 <b>${last.low.toFixed(2)}</b></span>
    <span class="${chg >= 0 ? 'up-c' : 'down-c'}">${chg >= 0 ? '+' : ''}${chg.toFixed(2)}%</span>
    <span style="color:#f5a623">— MA5</span>
    <span style="color:#4d8dff">— MA10</span>`;
}

async function load() {
  document.getElementById('klineMeta').textContent = '加载中…';
  let candles = await fetchKline(cur.code, cur.period, 90);
  let mocked = false;
  if (!candles) { candles = mockCandles(cur.code); mocked = true; }
  draw(candles);
  document.getElementById('klineMeta').textContent =
    `${cur.code.toUpperCase()} · ${candles.length} 根 · 前复权` + (mocked ? ' · 预览模拟数据' : ' · 东方财富');
}

export function showKline(code, name) {
  cur.code = code;
  cur.name = name || code.toUpperCase();
  document.getElementById('klineName').textContent = `${cur.name} · K线`;
  switchPage('kline');
  load();
}

export function initKline() {
  document.getElementById('klineBack').addEventListener('click', () => switchPage('watch'));
  document.querySelectorAll('.kp-btn').forEach(b => b.addEventListener('click', () => {
    document.querySelectorAll('.kp-btn').forEach(x => x.classList.toggle('active', x === b));
    cur.period = b.dataset.p;
    if (cur.code) load();
  }));
  if (!inTauri) console.log('[preview] K线走 mock 随机游走');
}

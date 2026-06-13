# rust-stock

<p align="center">
  <img src="logo.png" alt="rust-stock logo" width="150" />
</p>

<p align="center">
  <b>An entire hedge-fund investment committee, squeezed into a phone-sized floating window in the corner of your screen.</b><br/>
  Real-time A-share quotes · A five-school AI consensus engine · Supply-chain bottleneck mapping · Fully local · Desktop + Android
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="License: Apache-2.0"/></a>
  <img src="https://img.shields.io/badge/Tauri-2.x-24C8DB.svg" alt="Tauri 2"/>
  <img src="https://img.shields.io/badge/Rust-2021-orange.svg" alt="Rust"/>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Android-success.svg" alt="Platform"/>
  <img src="https://img.shields.io/badge/AI-DeepSeek%20%7C%20OpenAI--compatible-7C3AED.svg" alt="AI"/>
</p>

<p align="center">
  <a href="README.md">简体中文</a> · <b>English</b> · <a href="README.ja.md">日本語</a> · <a href="README.ko.md">한국어</a>
</p>

---

## ⚡ What it is

Other tools give you **one** AI opinion. rust-stock puts **five** investment schools — **Value · Growth · Momentum · Technical · Macro** — on the same stage to debate, then renders a **combined verdict** like a hedge-fund investment committee. And it all lives in a 360×640 frosted floating window in the corner of your desktop or phone, one that snaps to the screen edge and tucks away. Red-up / green-down (A-share convention), **fully local, zero servers** — your data never leaves your own device.

Desktop (Windows / macOS) and Android share **one codebase and one experience**.

> ⚠️ All AI output is **research ranking and idea reference only — it is not investment advice**. Markets carry risk; your decisions are your own.

## 🧠 The decision engine: not "ask an AI if it'll go up", but a methodology

The core of "Today's AI Picks" is a pipeline of **real data → multi-agent verdict → quote re-check**, blending two excellent public methodologies:

**① Local whole-market scan (real data, never fabricated by AI)**
Each day it first screens a candidate pool from real quotes on your own machine: **top gainers + top net main-capital inflow + Dragon-Tiger list**, merged and de-duplicated, with live price / change% / turnover / net main-capital / list status. The AI only ever sees hard, real numbers — cutting off "hallucinated quotes" at the source.

**② Supply-chain bottleneck research (the foundation) — inspired by [muxuuu/serenity-skill](https://github.com/muxuuu/serenity-skill)**
It translates market narratives into **systematic physical constraints**: breaking the chain into eight layers (downstream demand → system integration → modules → chips → process & packaging → equipment & testing → materials & consumables → infrastructure), pinpointing the **scarcest bottleneck layer** (supplier concentration / certification cycle / capacity-expansion difficulty / process barriers), then identifying who **controls** that layer.
> **Why**: themes get hyped and stories change, but capacity, yield and certification — physical bottlenecks — don't lie. Anchoring on real supply-chain constraints finds where value actually settles, instead of chasing sentiment.

**③ Multi-school consensus scoring (the verdict) — inspired by [virattt/ai-hedge-fund](https://github.com/virattt/ai-hedge-fund)**
For each candidate, five investment-school AI perspectives **score independently, then arbitrate**:

| School | What it watches |
|---|---|
| 💎 Value | Moat, valuation margin of safety |
| 🚀 Growth | Industry S-curve, TAM ceiling |
| 🔥 Momentum | Theme heat, Dragon-Tiger list, turnover energy |
| 📈 Technical | Trend, breakout, moving-average structure |
| 🌐 Macro | Policy, capital flows, liquidity |

> **Why**: the essence of ai-hedge-fund is "many investor agents + arbitration" — a single view always has blind spots; letting several professional schools debate first surfaces disagreement and exposes risk, far more reliable than "will it go up?".

**④ Per-stock deep research + real-quote re-check**
Two-stage output: an initial shortlist of 6–8, then **a separate eight-layer supply-chain deep-dive for each finalist** (same engine as the "Research" button: chain position / bottleneck & scarcity / five-school bull-bear / catalysts & validation / risk / falsification / research priority), folded into the rationale (run concurrently to control latency). All prices/changes are then **backfilled from real quotes** (AI is forbidden to invent numbers); hallucinated codes and names that **fell for six consecutive trading days (six straight down candles)** are dropped. Each pick comes with: **chain position / five-school disagreement / Dragon-Tiger capital signal / today's watch points / key risks / falsification conditions**.

> In one line: **real data for the base, supply-chain bottlenecks for direction, the five schools for selection, real quotes as backstop.** No limit-up hype, no stock tips — only evidence-based research ranking.

## 📊 What else

- **Market-sentiment gauge**: weighted change% of four major indices (SSE / SZSE Component / ChiNext / CSI 300) + tanh compression, mapped live to a -100–100 needle; click to flip in 3D for the calculation detail + AI read of the tape.
- **Watchlist AI check-up**: a mini gauge beside each watchlist stock, with an AI -100–+100 bull/bear score; tap for the full supply-chain rationale.
- **Pick thumbnails**: each recommended stock is followed by a real "last-30-day closing price" sparkline; one tap to the full daily K-line.
- **K-line / capital flow / chip distribution / technical indicators**: daily/weekly/monthly candles + MA5/MA10 + volume; TDX-convention MACD/KDJ/RSI/BOLL values with golden/death crosses; five-tier capital flow (main / super-large / large / medium / small orders, red in / green out); a **chip-distribution chart** — turnover decay + cost-center Gaussian kernel approximating chip pile-up at each price (red = profit / green = trapped), reading off profit ratio / average cost / current price.
- **Streaming AI chat + deep research + favorites**: bottom bar streams from DeepSeek token-by-token; the "Research" button enters the eight-layer supply-chain workflow; research conclusions can be **saved into groups**, copied whole or per-item, stored permanently on-device.
- **Off-hours notice**: when A-shares are pre-open / lunch break / closed / weekend, a red bar under the quote strip clearly notes "using previous trading day's data", so stale data is never mistaken for live.
- **Track-record backtest**: replays historical picks against K-line for win rate and per-trade return, letting the engine supervise itself.

## 🖼️ Screenshots

> Desktop + Android, one source; frosted dual theme (creamy white by day / pure black by night). Screenshots updated continuously.

| Market home | Watchlist + AI gauge | Today's AI Picks |
|:---:|:---:|:---:|
| ![Market](docs/screenshots/market.png) | ![Watchlist](docs/screenshots/watchlist.png) | ![Picks](docs/screenshots/recommend.png) |

| K-line + capital flow | Research favorites | Settings |
|:---:|:---:|:---:|
| ![K-line](docs/screenshots/kline.png) | ![Research](docs/screenshots/research.png) | ![Settings](docs/screenshots/settings.png) |

## ✨ Feature list

- **Floating-window experience (desktop)**: a borderless, rounded, always-on-top mini window; drag by the title bar; snaps to the screen edge automatically; click ✕ to collapse into a vertical gauge widget on the right, click again to expand; freely resizable.
- **Native Android**: the same UI runs as an Android app — pinch-zoom / pan on K-line, system back gesture to the previous page, full-screen-notch adaptation, branded launcher icon.
- **Dual quote sources**: Sina Finance / East Money, switchable and mutually redundant; seamless index ticker; add/remove watchlist by code or name/pinyin search.
- **Fully local persistence**: bundled SQLite — watchlist / settings / AI cache all on disk, single-file portable; no relay server, data stays on your device.
- **Plug any AI**: DeepSeek by default; Base URL / model can point to any OpenAI-compatible service (Kimi, Tongyi, local Ollama…); the key is stored only on your machine.
- **Liquid-glass look (experimental · optional in settings)**: an iOS "Liquid Glass"-style frosted skin — translucency + blur + highlight stroke + aurora background + floating pill tabs, day/night aware; off by default, auto-degrades to opaque on old Android devices.

Inspired by the fully-local form of [ArvinLovegood/go-stock](https://github.com/ArvinLovegood/go-stock) (Wails + Go), rebuilt with Tauri 2 (Rust + system WebView), with far smaller bundle and memory footprint than Electron-based apps.

## Quick start

### Environment (one-time)

- [Rust toolchain](https://rustup.rs) (Windows needs VS Build Tools "Desktop development with C++"; macOS needs Xcode CLT)
- Windows 10 may need the [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (built into Win11)
- Tauri CLI: `cargo install tauri-cli --version "^2"`

### Run

```bash
cd rust-stock
cargo tauri dev      # dev (frontend hot reload)
cargo tauri build    # bundle installer (Windows NSIS / macOS dmg)
```

### Frontend-only preview (no Rust)

```bash
cd rust-stock/src && python3 -m http.server 8080
# open http://localhost:8080 ; data is mocked
```

### Tests

```bash
cd rust-stock/src-tauri && cargo test
# covers: quote parsing (Sina/East Money), news parsing, sentiment algorithm, SQLite KV
```

## Configure AI (optional)

Enter an API key in Settings (default [DeepSeek](https://platform.deepseek.com); Base URL / model can point to any OpenAI-compatible service such as Kimi, Tongyi or local Ollama). The key is stored only in your local SQLite and connects directly. Once set, it enables watchlist AI scoring, AI sentiment reads and AI chat; without it, the app degrades gracefully with a prompt.

## Project structure

```
rust-stock/
├── src/                       # Frontend (vanilla HTML/CSS/JS + ES modules, no framework/build)
│   ├── index.html             # All UI (inline styles)
│   ├── main.js                # Bootstrap (wiring / timers)
│   └── js/
│       ├── bridge.js          # Tauri bridge (browser-preview fallback)
│       ├── store.js           # Global state + SQLite/localStorage persistence
│       ├── api.js             # Tauri command wrappers
│       ├── ui.js / router.js  # Common bits / page switching
│       └── pages/             # market / news / watchlist / chat / settings
├── src-tauri/                 # Rust local logic layer
│   ├── src/lib.rs             # Tauri command layer (business prompts / window control)
│   ├── src/sources/           # Quote source abstraction (QuoteSource trait + registry)
│   ├── src/ai.rs              # AI provider abstraction (OpenAI-compatible, base_url/model configurable)
│   ├── src/quote.rs           # Quote models & parsers (with unit tests)
│   ├── src/feed.rs            # News + sentiment algorithm (with unit tests)
│   ├── src/storage.rs         # SQLite KV persistence (with unit tests)
│   └── tauri.conf.json        # Window / bundle config
└── docs/                      # Dev docs
```

The full changelog lives in the **[Chinese README → 更新日志](README.md#更新日志)** (reverse chronological).

More details: [Development docs](rust-stock/docs/DEVELOPMENT.md)

## Disclaimer

Quote and news data come from third-party public APIs (Sina Finance, East Money) for study and research only; verify licensing before commercial use. AI analysis is model-generated, for reference only, and does not constitute investment advice. Investing carries risk — be cautious.

## Acknowledgments

- [ArvinLovegood/go-stock](https://github.com/ArvinLovegood/go-stock) — the inspiration for this project
- [Tauri](https://tauri.app) · [DeepSeek](https://deepseek.com)

## License

[Apache License 2.0](LICENSE)

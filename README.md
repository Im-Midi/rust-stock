# rust-stock

<p align="center">
  <img src="rust-stock/src-tauri/icons/128x128.png" alt="rust-stock logo" width="84" />
</p>

<p align="center">
  手机尺寸、可吸附屏幕边缘的悬浮股票行情助手<br/>
  Tauri 2 + Rust + 原生前端 · 纯本地运行 · 无服务器
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="License: Apache-2.0"/></a>
  <img src="https://img.shields.io/badge/Tauri-2.x-24C8DB.svg" alt="Tauri 2"/>
  <img src="https://img.shields.io/badge/Rust-2021-orange.svg" alt="Rust"/>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey.svg" alt="Platform"/>
</p>

---

## 它是什么

把臃肿的传统大窗口股票工具，改造成一个 **360×640 手机尺寸、置顶悬浮、拖到屏幕边缘自动吸附收起**的现代化小窗。深色纯净扁平风格，红涨绿跌（A 股习惯）。

参考 [ArvinLovegood/go-stock](https://github.com/ArvinLovegood/go-stock)（Wails + Go），本项目用 Tauri（Rust + 系统 WebView）重做：打包体积与内存占用远小于 Electron 系。

**核心原则：纯本地。** Rust 层只是本地逻辑（抓行情、调 AI、SQLite 读写），所有数据留在你电脑上，没有任何中转服务器。

## 界面预览

| 行情主页 | 自选股 + AI 打分 | AI 流式聊天 |
|:---:|:---:|:---:|
| ![行情主页](docs/screenshots/market.png) | ![自选股](docs/screenshots/watchlist.png) | ![AI聊天](docs/screenshots/chat.png) |

| 情绪翻面解读 | 7×24 快讯 | 设置 |
|:---:|:---:|:---:|
| ![情绪解读](docs/screenshots/sentiment-why.png) | ![快讯](docs/screenshots/news.png) | ![设置](docs/screenshots/settings.png) |

## 功能

- **悬浮窗体验**：无边框圆角置顶小窗，标题栏拖拽，拖到屏幕左右边缘自动吸附；点 ✕ 收起成 6px 小条，再点展开。窗口可自由缩放，UI 整体等比缩放
- **行情**：新浪财经 / 东方财富双数据源可切换，互为备份；指数无缝滚动条；自选股增删（支持 `600519` / `sh600519` 两种输入）
- **市场情绪表盘**：四大指数（上证/深成/创业板/沪深300）涨跌幅加权 + tanh 压缩，实时映射到 -100~100 指针；**点击表盘 3D 翻面**，背面是计算明细（每个指数的涨跌幅×权重）+ AI 结合盘面的解读
- **自选股 AI 打分**：每支自选股名称与价格之间有一个小仪表盘，DeepSeek 给出 -100（极度看跌）~ +100（极度看涨）综合打分，按天缓存；点击进入详情页看打分理由全文。未接入 AI 时指针停 0 并提示
- **AI 流式聊天**：底栏输入框直连 DeepSeek（SSE 流式，逐字出现），保留对话上下文
- **快讯**：东方财富 7×24 全球直播真实数据，60s 自动刷新，标红要闻带标签
- **本地持久化**：SQLite（rusqlite bundled），自选股/设置/AI 缓存全部落库，数据库单文件可拷走迁移
- **今日 AI 推荐**：每天 AI 结合当日情绪与指数详尽分析后推荐 3 支（基本面/技术面/消息面+风险提示）；同一支连续 ≥7 个推荐日出现会加 ★ 并注明天数；点击看分析全文
- **K线图**：自选股点名称进入，日/周/月K 蜡烛图（前复权）+ MA5/MA10 + 成交量，canvas 绘制零依赖
- **窗口记忆与自启**：记住上次窗口位置和大小；设置页一键开关开机自启动
- **右侧仪表盘挂件**：点最小化，主窗缩为屏幕右缘的竖排小挂件——每支自选股一个 AI 表盘 + 名称 + 涨跌幅；顶部把手可拖到任意位置，点击即还原主窗

## 快速上手

### 环境（一次性）

- [Rust 工具链](https://rustup.rs)（Windows 需 VS Build Tools 的"使用 C++ 的桌面开发"；macOS 需 Xcode CLT）
- Windows 10 可能需装 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Win11 自带）
- Tauri CLI：`cargo install tauri-cli --version "^2"`

### 运行

```bash
cd rust-stock
cargo tauri dev      # 开发调试（前端热重载）
cargo tauri build    # 打包安装包（Windows NSIS / macOS dmg）
```

### 纯前端预览（不装 Rust）

```bash
cd rust-stock/src && python3 -m http.server 8080
# 浏览器开 http://localhost:8080 ，数据走 mock
```

### 跑测试

```bash
cd rust-stock/src-tauri && cargo test
# 覆盖：行情解析（新浪/东财）、快讯解析、情绪算法、SQLite KV
```

## 配置 AI（可选）

设置页填入 API Key（默认 [DeepSeek](https://platform.deepseek.com)；Base URL / 模型可改成任意 OpenAI 兼容服务，如 Kimi、通义、本地 Ollama）。key 只存你本机 SQLite，本地直连。填入后自动启用：自选股 AI 打分、情绪 AI 解读、AI 聊天；不填则优雅降级并提示。

## 工程结构

```
rust-stock/
├── src/                       # 前端（原生 HTML/CSS/JS + ES modules，无框架无构建）
│   ├── index.html             # 全部 UI（样式内联）
│   ├── main.js                # 入口 bootstrap（接线/定时器）
│   └── js/
│       ├── bridge.js          # Tauri 桥接（浏览器预览降级）
│       ├── store.js           # 全局状态 + SQLite/localStorage 持久化
│       ├── api.js             # Tauri 命令封装
│       ├── ui.js / router.js  # 通用件 / 页面切换
│       └── pages/             # 行情 / 快讯 / 自选 / 聊天 / 设置
├── src-tauri/                 # Rust 本地逻辑层
│   ├── src/lib.rs             # Tauri 命令层（业务 prompt / 窗口控制）
│   ├── src/sources/           # 行情数据源抽象（QuoteSource trait + 注册表）★新增源在此加一行
│   ├── src/ai.rs              # AI Provider 抽象（OpenAI 兼容协议，base_url/model 可配）
│   ├── src/quote.rs           # 行情模型与解析器（含单测）
│   ├── src/feed.rs            # 快讯 + 情绪算法（含单测）
│   ├── src/storage.rs         # SQLite KV 持久化（含单测）
│   └── tauri.conf.json        # 窗口/打包配置
└── docs/                      # 开发文档 / 项目记忆 / 避坑记录
```

更多细节：[开发文档](rust-stock/docs/DEVELOPMENT.md) · [项目记忆](rust-stock/docs/MEMORY.md) · [避坑记录](rust-stock/docs/PITFALLS.md)

## Roadmap

- [ ] 板块热力接真实数据（当前为演示数据）
- [x] 数据源抽象为 trait，新增数据源即插即用（`sources/` 注册表）
- [x] 前端模块化拆分（原生 ES modules，无构建依赖）
- [x] GitHub Actions CI（cargo test + 前端语法检查）
- [x] 记住窗口位置和大小 / 开机自启动开关
- [ ] 系统托盘
- [x] K线图（日/周/月K + MA + 成交量）
- [ ] 个股详情页（盘口/资金流）

## 声明

行情与快讯数据来自第三方公开接口（新浪财经、东方财富），仅供学习研究，商用前请自行确认数据源授权。AI 分析内容由大模型生成，仅供参考，不构成投资建议。投资有风险，入市需谨慎。

## 致谢

- [ArvinLovegood/go-stock](https://github.com/ArvinLovegood/go-stock) — 本项目的灵感来源
- [Tauri](https://tauri.app) · [DeepSeek](https://deepseek.com)

## License

[Apache License 2.0](LICENSE)

# rust-stock · 开发文档

> 一个手机尺寸、可吸附屏幕边缘的悬浮股票行情助手。纯本地桌面软件，无服务器。

## 1. 项目定位

把臃肿的传统大窗口股票工具，改造成一个 **360×640 手机尺寸、可拖到屏幕边缘吸附收起** 的现代化悬浮窗。深色纯净扁平风格。

参考项目 `ArvinLovegood/go-stock`（Wails + Go + Vue + NaiveUI），但本项目用 **Tauri（Rust + WebView）** 重做。两者同属"原生本地进程 + WebView 前端"形态。

**核心原则：纯本地，无后端服务器。** Rust 层是本地逻辑层（抓行情、调 AI、本地存储），所有数据留在用户电脑上。

## 2. 技术栈

| 层 | 技术 | 说明 |
|---|---|---|
| 壳 | Tauri 2 | 跨平台桌面框架，原生进程 + 系统 WebView |
| 本地逻辑层 | Rust | 行情抓取、AI 调用、窗口控制 |
| 前端 | 原生 HTML/CSS/JS | 无框架，轻量。后续可换 Vue/React |
| 字体 | Sora（标题）+ DM Mono（数字） | Google Fonts |
| 行情数据 | 新浪财经 / 东方财富 公开接口 | 第三方，本地直连 |
| AI | DeepSeek API | 用户自带 key，本地直连 |

## 3. 工程结构

```
rust-stock/
├── docs/                      # 文档（开发/记忆/避坑）
│   ├── DEVELOPMENT.md         # 本文件
│   ├── MEMORY.md              # 项目记忆：决策、状态、待办
│   └── PITFALLS.md            # 避坑文档
├── src/                       # 前端（WebView 加载的内容）
│   ├── index.html             # 悬浮窗 UI（全部样式内联在 <style>）
│   └── main.js                # 数据渲染 + Tauri 桥接 + mock 回退
├── src-tauri/                 # Rust 本地逻辑层
│   ├── src/
│   │   ├── lib.rs             # 主逻辑：窗口控制 + 命令注册
│   │   ├── main.rs            # 入口（仅调用 lib::run）
│   │   └── quote.rs           # 行情抓取模块（新浪+东方财富，含单元测试）
│   ├── Cargo.toml             # Rust 依赖
│   ├── build.rs               # tauri-build
│   └── tauri.conf.json        # 窗口配置（无边框/透明/置顶/手机尺寸）
└── README.md                  # 快速上手
```

## 4. 如何运行

### 前端预览（最快，无需 Rust）
```bash
cd src && python3 -m http.server 8080
# 浏览器开 http://localhost:8080/index.html
```
此模式下数据走 mock（main.js 里的 indices/heat/news）。
⚠️ 必须用 http server，直接双击 file:// 会被浏览器 CORS 拦截 ES module。

### 桌面应用（完整功能）
前提：Rust 工具链 + Tauri CLI 2.x
```bash
cargo install tauri-cli --version "^2"
cd rust-stock
cargo tauri dev      # 开发调试，热重载
cargo tauri build    # 打包 Windows .msi/.exe，Mac .dmg
```

### 跑测试（验证行情解析）
```bash
cd src-tauri
cargo test           # 运行 quote.rs 的 4 个解析单元测试
```

## 5. 各模块职责

### `src/index.html`
单文件 UI，所有 CSS 在 `<style>` 里，用 CSS variables 管理主题色。结构：
- `.titlebar`：自绘标题栏，`data-tauri-drag-region` 让它可拖拽整窗
- `.ticker`：顶部指数滚动条（CSS 动画无缝滚动）
- `.body`：可滚动主体 —— 情绪仪表盘（SVG）、板块热力网格、快讯流
- `.dock`：底部 AI 输入框 + 4 个导航 tab

配色关键：`--up:#ff4d4f`(红涨) / `--down:#14c87d`(绿跌)，符合 A 股习惯。

### `src/main.js`
- `inTauri` 检测运行环境，`tauriInvoke()` 在浏览器下安全降级为 console.log
- `loadIndices()`：Tauri 下调 `fetch_quotes` 拉真实行情，失败/浏览器回退 mock
- `renderTicker/renderHeat/renderFeed`：渲染三块内容
- 窗口按钮（置顶/最小化/收起）绑定到对应 Tauri 命令
- 行情每 10s 轮询刷新

### `src-tauri/src/lib.rs`
Tauri 命令（前端通过 `invoke` 调用）：
- `set_always_on_top(pinned)` — 置顶切换
- `minimize_window()` — 最小化
- `toggle_dock_edge()` — 收起到边缘 / 展开
- `ask_ai(question)` — AI 提问（TODO：接 DeepSeek）
- `fetch_quotes(source, codes)` — 抓行情
- `snap_to_edge()`（非命令，窗口事件回调）— 拖拽松手后自动吸边

### `src-tauri/src/quote.rs`
行情抓取核心，**已通过单元测试**：
- `parse_sina_response()` — 新浪 GBK 文本解析
- `parse_eastmoney_response()` — 东方财富 JSON 解析（×100 倍率还原）
- `fetch_sina/fetch_eastmoney()` — 异步网络抓取（`net` feature）
- 统一输出 `Quote` 结构体

## 6. 数据流

```
[新浪/东方财富接口] --HTTP--> [Rust quote.rs 解析] --invoke--> [main.js 渲染] --> [WebView UI]
                                       ↑                            ↓
                              纯本地，无服务器           10s 轮询刷新
```

## 7. 当前完成度

- ✅ UI 框架（手机尺寸、深色扁平、三大模块 + AI 输入 + 导航）
- ✅ 窗口控制（拖拽、置顶、最小化、边缘吸附逻辑）
- ✅ 行情抓取模块（双源，解析逻辑经单元测试验证）
- ✅ 前端接入真实行情 + mock 回退
- ⬜ DeepSeek 流式接入（ask_ai 是桩）
- ⬜ SQLite 本地存储（自选股/设置/历史）
- ⬜ 情绪/热力接真实数据（目前 mock）
- ⬜ 快讯接真实数据源（财联社等）
- ⬜ 多 tab 实际页面（目前只有"行情"页有内容）
- ⬜ 系统托盘、设置页

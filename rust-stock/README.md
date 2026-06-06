# rust-stock · 可吸附边缘的悬浮行情助手

把原来臃肿的大窗口改造成手机尺寸（360×640）、可拖到屏幕边缘吸附收起的现代化悬浮窗。深色纯净扁平风格。

## 架构：纯本地，无服务器

参考 ArvinLovegood/go-stock（Wails + Go），本项目用 **Tauri（Rust + WebView）**，同属"原生本地进程 + WebView 前端"形态。**Rust 这一层是本地逻辑层，不是服务器** —— 它在用户电脑上直接抓行情、调 DeepSeek、读写本地存储。所有数据留在本地，无任何中转服务。

## 目录结构

```
rust-stock/
├── src/                  # 前端（你现有的 WebView 内容替换这里）
│   ├── index.html        # 悬浮窗 UI
│   └── main.js           # 数据渲染 + Tauri 桥接
└── src-tauri/
    ├── src/
    │   ├── lib.rs        # 窗口控制 + 边缘吸附核心逻辑
    │   └── main.rs       # 入口
    ├── Cargo.toml
    ├── build.rs
    └── tauri.conf.json   # 无边框/透明/置顶/手机尺寸配置
```

## 本地预览（纯前端，无需 Rust）

```bash
cd src && python3 -m http.server 8080
# 浏览器打开 http://localhost:8080/index.html
```
注意：必须走 http server，直接双击 file:// 会被 CORS 拦截 module 脚本。

## 跑成真正的桌面悬浮窗

前提：已装 Rust 工具链 + Tauri CLI（`cargo install tauri-cli --version "^2"`）。

```bash
cd rust-stock
cargo tauri dev      # 开发调试
cargo tauri build    # 打包 Windows .exe / .msi（也支持 Mac .dmg）
```

## 关键实现说明

### 1. 手机尺寸 + 无边框（tauri.conf.json）
- `width:360, height:640, maxWidth:360, resizable:false` → 锁死手机比例
- `decorations:false` → 去掉系统标题栏，用自绘的拖拽栏
- `transparent:true` + `shadow:true` → 圆角 + 投影
- `alwaysOnTop:true` → 默认置顶

### 2. 拖拽
HTML 里 `data-tauri-drag-region` 属性的元素（顶部标题栏）可拖动整个窗口，无需写 JS。

### 3. 边缘吸附（lib.rs · snap_to_edge）
监听 `WindowEvent::Moved`，松手后若窗口离左/右屏幕边缘 < 24px 就自动贴齐。
> 生产环境建议加防抖：Moved 在拖拽中会高频触发，可只在拖拽结束（用一个定时器 100ms 无新事件）时执行 snap，避免拖拽卡顿。

### 4. 收起 / 展开（toggle_dock_edge）
点标题栏的 × 按钮触发：把窗口推出屏幕外、只留 6px 小条；再点展开。可改成鼠标移到小条上自动展开（监听 mouseenter）。

### 5. 本地行情抓取（quote.rs）—— 已实现并通过单元测试
双数据源，Rust 进程直连公开接口，纯本地：
- **新浪** `hq.sinajs.cn`：必须带 `Referer: https://finance.sina.com.cn`，返回 GBK 需转码（代码已用 encoding_rs 处理）
- **东方财富** `push2.eastmoney.com`：UTF-8 JSON，价格带 ×100 倍率（代码已还原）
- 前端 `main.js` 的 `loadIndices()` 在 Tauri 下调 `fetch_quotes` 命令，浏览器预览自动回退 mock
- 行情每 10s 轮询刷新（`setInterval`）；交易时段外建议调长

**验证**：`cd src-tauri && cargo test` 跑 quote 模块的 4 个解析测试（新浪/东方财富/空数据/异常容错），已全部通过。
> 注意：第三方接口字段顺序偶有调整，首次本地运行时 `println!` 看一眼真实返回，按需微调 quote.rs 里的字段索引即可。这些接口的稳定性与商用许可由数据源方决定，商用前请自行确认。

### 6. AI 提问（ask_ai 命令）—— 本地直连 DeepSeek
用户在设置里填自己的 DeepSeek API key，Rust 进程拿 key 直接调，无中转。
流式输出建议用 Tauri event：后端 `window.emit("ai-chunk", text)`，前端 `listen("ai-chunk", ...)` 逐段追加到气泡。

## 配色规范（CSS variables，index.html :root）
- 涨 `--up:#ff4d4f`（红）/ 跌 `--down:#14c87d`（绿）—— A股习惯
- 主强调 `--accent:#4d8dff`
- 背景层次 `--bg → --surface → --surface-2/3`，靠明度区分卡片层级

## 可继续做的
- 系统托盘图标（Cargo.toml 已开 `tray-icon` feature）：最小化到托盘、右键菜单
- 鼠标悬停小条自动展开 / 移开 3s 自动收起
- 多 tab 切换的实际页面（目前只有「行情」页有内容）
- 记住上次窗口位置（写入本地配置）

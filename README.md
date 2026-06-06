# rust-stock

手机尺寸、可吸附屏幕边缘的悬浮股票行情助手。Tauri 2 + Rust + 原生前端，纯本地运行、无服务器。

工程代码在 [`rust-stock/`](rust-stock/)，快速上手见 [`rust-stock/README.md`](rust-stock/README.md)。

## 功能

- 360×640 无边框置顶悬浮窗，拖到屏幕边缘自动吸附收起，窗口自由缩放（UI 等比）
- 行情：新浪 / 东方财富双数据源，指数滚动条 + 自选股
- 市场情绪表盘：四大指数加权实时计算，点击翻面看计算明细与 AI 解读
- 自选股 AI 看涨/看跌打分（-100~100 小仪表盘），点击看 AI 分析全文
- AI 流式聊天（DeepSeek，用户自带 key，本地直连）
- 快讯：东方财富 7×24 真实数据
- 本地 SQLite 持久化（自选/设置/AI 缓存）

## 文档

- [开发文档](rust-stock/docs/DEVELOPMENT.md) · [项目记忆](rust-stock/docs/MEMORY.md) · [避坑记录](rust-stock/docs/PITFALLS.md)

## 声明

行情与快讯数据来自第三方公开接口，仅供学习研究，商用前请自行确认数据源授权。AI 分析内容仅供参考，不构成投资建议。投资有风险，入市需谨慎。

## License

[Apache-2.0](LICENSE)

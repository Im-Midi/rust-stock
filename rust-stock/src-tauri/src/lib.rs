// rust-stock — Tauri 本地逻辑层（无服务器，全部本地运行）
// 架构：
//   sources/  行情数据源抽象（QuoteSource trait + 注册表，新增源零改动）
//   ai.rs     AI Provider 抽象（OpenAI 兼容协议，base_url/model 可配，默认 DeepSeek）
//   feed.rs   快讯 + 市场情绪算法
//   storage.rs SQLite KV 持久化
//   quote.rs  行情数据模型与解析器（含单测）

mod ai;
mod feed;
mod kline;
mod quote;
mod sources;
mod storage;

use quote::Quote;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow,
};

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(band) = app.get_webview_window("band") {
        let _ = band.hide();
    }
}

const EDGE_THRESHOLD: f64 = 24.0; // 距屏幕边缘多少像素触发吸附
const PEEK_WIDTH: f64 = 6.0; // 收起后露出的小条宽度

// ============================================================
// 行情
// ============================================================

/// 列出已注册的数据源（设置页下拉框动态生成）
#[tauri::command]
fn list_sources() -> Vec<sources::SourceInfo> {
    sources::list()
}

/// 股票搜索（名称/代码/拼音首字母），自选页添加用
#[tauri::command]
async fn search_stocks(keyword: String) -> Result<Vec<sources::SearchHit>, String> {
    let kw = keyword.trim();
    if kw.is_empty() {
        return Ok(vec![]);
    }
    sources::search_stocks(kw).await
}

/// 抓取行情。codes 用统一格式（sh600519 / int_dji），各源内部转换
#[tauri::command]
async fn fetch_quotes(source: String, codes: Vec<String>) -> Result<Vec<Quote>, String> {
    sources::get(&source)
        .ok_or_else(|| format!("未知数据源: {source}"))?
        .fetch(&codes)
        .await
}

#[tauri::command]
async fn fetch_news() -> Result<Vec<feed::NewsItem>, String> {
    feed::fetch_news().await
}

#[tauri::command]
async fn fetch_sentiment() -> Result<feed::Sentiment, String> {
    feed::fetch_sentiment().await
}

/// 自选股相关快讯（行情页"自选股信息"卡片）
#[tauri::command]
async fn fetch_stock_news(codes: Vec<String>) -> Result<Vec<feed::NewsItem>, String> {
    feed::fetch_stock_news(&codes).await
}

/// 历史K线。period: "day" | "week" | "month"
#[tauri::command]
async fn fetch_kline(code: String, period: String, count: u32) -> Result<Vec<kline::Candle>, String> {
    let klt = match period.as_str() {
        "week" => 102,
        "month" => 103,
        _ => 101,
    };
    kline::fetch_kline(&code, klt, count.clamp(20, 300)).await
}

// ============================================================
// 本地存储（SQLite KV）
// ============================================================

#[tauri::command]
fn db_get(db: tauri::State<storage::Db>, key: String) -> Result<Option<String>, String> {
    storage::kv_get(&db, &key)
}

#[tauri::command]
fn db_set(db: tauri::State<storage::Db>, key: String, value: String) -> Result<(), String> {
    storage::kv_set(&db, &key, &value)
}

// ============================================================
// AI 命令（业务 prompt 在这里，协议细节在 ai.rs）
// ============================================================

/// 深度调研模式系统 prompt：供应链瓶颈研究法（产业链八层拆解，先排层级再排公司）
const RESEARCH_SYSTEM: &str = "你是嵌在 rust-stock 里的A股产业链深度调研助手。收到主题后按以下工作流推理：\
1) 把市场叙事翻译成系统性变化：什么技术/经济变化在驱动需求，哪个物理约束最关键（功耗/带宽/良率/纯度/产能/认证）；\
2) 拆解产业链八层：下游需求→系统集成→模块子系统→芯片器件→制程封装→设备检测→材料耗材→基础设施；\
3) 先排层级再排公司：指出哪几层最接近真实扩产约束（稀缺层），理由落在供应商数量、认证周期、扩产难度、工艺壁垒；\
4) 列每个关键层的代表性A股公司并分类：控制稀缺层/供应稀缺层/受益于主题/仅有故事；\
5) 给出优先研究清单（3~7家），每家按「卡住的环节/产业链位置（上游是什么、下游卖给谁）/排序原因（需求传导到哪条财务线）/应核实的证据方向（年报、公告、互动易、招投标、环评、客户认证）/主要风险/证伪条件」展开；\
6) 指出一个市场热捧但你排序靠后的方向并解释为什么；\
7) 结尾给「下一步核实清单」。\
规则：你没有实时行情和新闻检索，禁止编造具体价格/市值/涨跌幅/合同金额；结尾必须注明『以上基于模型知识推理，未经实时新闻与公告核验，请按核实清单自行验证』；只做研究排序，不做买卖指令。中文回答，分节清晰。";

/// 流式聊天：逐 delta emit "ai-chunk"，结束 "ai-done"，错误 "ai-error"
/// mode = "research" 时启用深度调研工作流
#[tauri::command]
async fn ask_ai(
    window: WebviewWindow,
    key: String,
    base_url: Option<String>,
    model: Option<String>,
    question: String,
    history: Vec<serde_json::Value>,
    mode: Option<String>,
) -> Result<(), String> {
    let cfg = ai::AiConfig::new(key, base_url, model)?;
    let deep = mode.as_deref() == Some("research");
    let system = if deep {
        RESEARCH_SYSTEM
    } else {
        "你是嵌在悬浮行情助手 rust-stock 里的股票 AI。回答简洁（手机宽度的窗口），中文，涉及操作建议时务必提示风险、注明仅供参考。"
    };
    let temperature = if deep { 0.4 } else { 0.6 };
    let mut messages = vec![serde_json::json!({ "role": "system", "content": system })];
    messages.extend(history);
    messages.push(serde_json::json!({ "role": "user", "content": question }));

    let result = ai::chat_stream(&cfg, messages, temperature, |delta| {
        let _ = window.emit("ai-chunk", delta);
    })
    .await;

    match result {
        Ok(()) => {
            let _ = window.emit("ai-done", ());
            Ok(())
        }
        Err(e) => {
            let _ = window.emit("ai-error", &e);
            Err(e)
        }
    }
}

#[derive(serde::Serialize)]
pub struct AiAnalysis {
    pub score: i32,       // -100(极度看跌) ~ 100(极度看涨)
    pub analysis: String, // 打分理由
}

/// 个股看涨/看跌打分
#[tauri::command]
async fn analyze_stock(
    key: String,
    base_url: Option<String>,
    model: Option<String>,
    name: String,
    code: String,
    price: f64,
    change_pct: f64,
) -> Result<AiAnalysis, String> {
    let cfg = ai::AiConfig::new(key, base_url, model)?;
    let prompt = format!(
        "对A股股票「{name}」（代码 {code}，现价 {price:.2}，今日涨跌 {change_pct:+.2}%——这两个是真实行情数字，可以引用）\
         做详尽的产业链式多空判断。严格只输出 JSON：\
         {{\"score\": -100到100的整数（越看涨越接近100，越看跌越接近-100，中性为0）, \
         \"analysis\": \"300~450字，必须依次覆盖六个小节并用「」标出：\
         「产业链位置」它在哪条产业链的哪一层，上游供给它什么、下游卖给谁；\
         「卡点判断」它是控制稀缺环节、供应稀缺环节、还是仅受益于主题——稀缺性的来源（认证周期/扩产难度/工艺壁垒）；\
         「误分类检验」市场现在把它当什么标签，它可能正在变成什么；\
         「短线多空」结合给定的真实涨跌幅判断当前位置与情绪；\
         「验证指标」未来1~4个季度看哪些财报/公告信号能确认或推翻判断；\
         「证伪条件」出现什么情况说明这个判断错了。\
         除给定的现价与涨跌幅外，禁止编造其他具体数字。\"}}"
    );
    let messages = vec![
        serde_json::json!({ "role": "system", "content": "你是严谨的股票分析助手。只输出 JSON，不输出任何其他文字。分析仅供参考，不构成投资建议。" }),
        serde_json::json!({ "role": "user", "content": prompt }),
    ];
    let content = ai::chat_once(&cfg, messages, 0.3).await?;
    let parsed = ai::extract_json(&content)?;
    Ok(AiAnalysis {
        score: parsed["score"].as_i64().unwrap_or(0).clamp(-100, 100) as i32,
        analysis: parsed["analysis"].as_str().unwrap_or("").to_string(),
    })
}

/// 解读市场情绪为何处于当前档位（点击表盘翻面时调用）
#[tauri::command]
async fn explain_sentiment(
    key: String,
    base_url: Option<String>,
    model: Option<String>,
    score: f64,
    label: String,
    detail: String,
) -> Result<String, String> {
    let cfg = ai::AiConfig::new(key, base_url, model)?;
    let prompt = format!(
        "当前A股市场情绪指标 {score:.1}（区间 -100 极度恐慌 ~ +100 极度乐观），档位「{label}」。\
         指标由主要指数涨跌幅加权得出：{detail}。\
         请用150字以内中文解释为什么市场情绪处于这个档位：结合各指数表现，推测可能的宏观/资金面因素。\
         直接输出正文（不要 markdown），结尾注明仅供参考。"
    );
    let messages = vec![
        serde_json::json!({ "role": "system", "content": "你是简洁严谨的市场分析助手。" }),
        serde_json::json!({ "role": "user", "content": prompt }),
    ];
    ai::chat_once(&cfg, messages, 0.4).await
}

/// 今日 AI 推荐：详尽分析后推荐 3 支
#[derive(serde::Serialize)]
pub struct RecStock {
    pub code: String,
    pub name: String,
    pub score: i32,
    pub reason: String,
    pub price: f64,      // 真实现价（复核行情）
    pub change_pct: f64, // 真实当日涨跌幅
}

#[tauri::command]
async fn ai_recommend(
    key: String,
    base_url: Option<String>,
    model: Option<String>,
    context: String, // 前端拼好的盘面背景（日期/情绪/指数）
) -> Result<Vec<RecStock>, String> {
    let cfg = ai::AiConfig::new(key, base_url, model)?;
    let prompt = format!(
        "{context}。请用供应链瓶颈研究法给出 6 支当前值得【买入关注】的A股候选\
        （系统随后会用真实实时行情复核，自动淘汰当日明显下跌的标的，最终展示前 3 支）。\
         研究方法：先把主题翻译成系统性变化，拆解产业链上下游（下游需求→系统集成→模块→芯片器件→制程封装→设备检测→材料耗材→基础设施），\
         找出供给最难扩张的稀缺层（供应商少/认证周期长/扩产难/工艺壁垒高），优先推荐控制或最接近稀缺层的公司，而非单纯蹭主题的公司。\
         硬性要求：\
         1) 只选看涨标的——看跌、破位、抛压沉重的绝不能出现；\
         2) 避开 ST、退市风险股；代码必须真实存在；\
         3) 你无法获取实时行情，禁止编造当日涨跌幅、现价、市值、合同金额等数字；\
         4) 每支 reason 必须详尽（250~400字），依次覆盖六个小节并用「」标出：\
         「产业链位置」处于哪条链的哪一层，上游是什么、下游卖给谁；\
         「卡住的环节」控制/供应/受益于哪个稀缺环节，为什么难扩产；\
         「排序原因」需求传导到它的财务路径（收入/毛利/订单哪一项先动）；\
         「证据」公开可查的支撑方向（公告/订单/产能/客户认证，不编造具体数字）；\
         「主要风险」替代技术/竞争扩产/需求不及预期/估值透支中最致命的一条；\
         「证伪条件」什么情况说明判断错了。\
         严格只输出 JSON 数组，共 6 个元素：\
         [{{\"code\":\"sh600519 或 sz000001 这种格式\",\"name\":\"股票名称\",\
         \"score\":1到100的整数（看涨信心，不允许负数）,\"reason\":\"…\"}}]"
    );
    let messages = vec![
        serde_json::json!({ "role": "system", "content": "你是严谨的A股产业链研究助手，方法论：供应链瓶颈优先，先排层级再排公司。只输出 JSON 数组，不输出任何其他文字。推荐仅供参考，不构成投资建议。" }),
        serde_json::json!({ "role": "user", "content": prompt }),
    ];
    let content = ai::chat_once(&cfg, messages, 0.5).await?;
    let arr = ai::extract_json_array(&content)?;
    let list = arr.as_array().ok_or("AI 返回的不是数组")?;
    let mut cands: Vec<(String, String, i32, String)> = Vec::new();
    for item in list.iter().take(6) {
        let raw_code = item["code"].as_str().unwrap_or("").trim().to_lowercase();
        // 容错：AI 可能只回 6 位数字
        let code = if raw_code.len() == 6 && raw_code.chars().all(|c| c.is_ascii_digit()) {
            let prefix = if raw_code.starts_with('6') || raw_code.starts_with('5') || raw_code.starts_with('9') { "sh" } else { "sz" };
            format!("{prefix}{raw_code}")
        } else {
            raw_code
        };
        if !(code.len() == 8 && (code.starts_with("sh") || code.starts_with("sz"))) {
            continue; // 格式不合法的丢弃
        }
        // 只收看涨标的：AI 若违规给出 0/负分（看跌），直接丢弃
        let score = item["score"].as_i64().unwrap_or(0);
        if score <= 0 {
            continue;
        }
        cands.push((
            code,
            item["name"].as_str().unwrap_or("").to_string(),
            score.clamp(1, 100) as i32,
            item["reason"].as_str().unwrap_or("").to_string(),
        ));
    }
    if cands.is_empty() {
        return Err("AI 未返回符合要求的看涨推荐，请点「重新生成」".into());
    }

    // ===== 真实行情复核 =====
    // AI 没有实时数据，推荐与个股打分可能打架。这里用真实行情把关：
    //   1) 当日跌幅超过 1.5% 的候选淘汰（"推荐却在跌"的来源）
    //   2) 行情里查无此码的淘汰（AI 编造的代码）
    //   3) 剩余按 score 排序取前 3，并带回真实价格/涨跌幅供前端展示
    let mut out: Vec<RecStock> = Vec::new();
    let mut market_checked = false;
    if let Some(src) = sources::get("eastmoney") {
        let codes: Vec<String> = cands.iter().map(|c| c.0.clone()).collect();
        if let Ok(quotes) = src.fetch(&codes).await {
            market_checked = true;
            for (code, name, score, reason) in &cands {
                // 东财返回的 code 是 6 位数字（无 sh/sz 前缀），按后缀匹配
                let Some(q) = quotes.iter().find(|q| !q.code.is_empty() && code.ends_with(&q.code)) else {
                    continue; // 查无此码 → AI 幻觉代码，丢弃
                };
                if q.change_pct < -1.5 {
                    continue; // 当日明显下跌 → 不推
                }
                out.push(RecStock {
                    code: code.clone(),
                    name: if name.is_empty() { q.name.clone() } else { name.clone() },
                    score: *score,
                    reason: reason.clone(),
                    price: q.price,
                    change_pct: q.change_pct,
                });
            }
            out.sort_by(|a, b| b.score.cmp(&a.score));
            out.truncate(3);
        }
    }
    if !market_checked {
        // 行情服务不可用：退化为未复核的前 3（price=0 表示未复核）
        out = cands
            .into_iter()
            .take(3)
            .map(|(code, name, score, reason)| RecStock { code, name, score, reason, price: 0.0, change_pct: 0.0 })
            .collect();
    }
    if out.is_empty() {
        return Err("AI 候选经实时行情复核后全部被淘汰（当日均在下跌或代码无效），今天可能不宜追高，请稍后再试".into());
    }
    Ok(out)
}

// ============================================================
// 窗口控制
// ============================================================

#[tauri::command]
fn set_always_on_top(window: WebviewWindow, pinned: bool) {
    let _ = window.set_always_on_top(pinned);
}

#[tauri::command]
fn minimize_window(window: WebviewWindow) {
    let _ = window.minimize();
}

/// 最小化到系统托盘：藏主窗（托盘图标常驻，点图标/菜单可还原）
#[tauri::command]
fn hide_to_tray(window: WebviewWindow) {
    let _ = window.hide();
    if let Some(band) = window.app_handle().get_webview_window("band") {
        let _ = band.hide();
    }
}

/// 彻底退出应用
#[tauri::command]
fn quit_app(window: WebviewWindow) {
    save_win_state(&window); // 退出前存一次窗口位置/大小
    window.app_handle().exit(0);
}

const BAND_W: f64 = 86.0; // 挂件逻辑宽度

/// 缩小为屏幕右缘的竖排仪表盘挂件：隐藏主窗，band 窗贴右边显示。
/// 挂件顶部有拖把手，用户可自行拖到任意位置。
#[tauri::command]
fn show_band(window: WebviewWindow) {
    let app = window.app_handle();
    let Some(band) = app.get_webview_window("band") else { return };
    // 已经显示过且被用户拖过 → 保持原位，只重新显示
    let first_show = !band.is_visible().unwrap_or(false);
    if first_show {
        if let Ok(Some(mon)) = window.current_monitor() {
            let wa = mon.work_area();
            let scale = mon.scale_factor();
            let w = (BAND_W * scale) as u32;
            let h = (320.0 * scale) as u32;
            let _ = band.set_size(PhysicalSize::new(w, h));
            let _ = band.set_position(PhysicalPosition::new(
                wa.position.x + wa.size.width as i32 - w as i32 - (8.0 * scale) as i32,
                wa.position.y + (wa.size.height as i32 - h as i32) / 2,
            ));
        }
    }
    let _ = band.show();
    let _ = band.set_always_on_top(true);
    let _ = window.hide();
}

/// 挂件按自选数量自适应高度（band 前端渲染完调用，逻辑像素）
#[tauri::command]
fn resize_band(window: WebviewWindow, height: f64) {
    let Ok(Some(mon)) = window.current_monitor() else { return };
    let scale = mon.scale_factor();
    let w = (BAND_W * scale) as u32;
    let h = (height.clamp(70.0, 720.0) * scale) as u32;
    let _ = window.set_size(PhysicalSize::new(w, h));
}

/// 从横幅点击还原主窗
#[tauri::command]
fn restore_main(window: WebviewWindow) {
    let app = window.app_handle();
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        clamp_to_workarea(&main); // 防止还原到屏外位置（看起来像"点了没反应"）
        let _ = main.set_focus();
    }
    let _ = window.hide(); // band 自己藏起来
}

/// 吸附到最近的屏幕边缘 / 展开
#[tauri::command]
fn toggle_dock_edge(window: WebviewWindow) {
    let monitor = match window.current_monitor() {
        Ok(Some(m)) => m,
        _ => return,
    };
    let screen = monitor.size();
    let scale = monitor.scale_factor();
    let pos = window.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
    let size = window.outer_size().unwrap_or(PhysicalSize::new(360, 640));

    let win_w = size.width as f64;
    let center_x = pos.x as f64 + win_w / 2.0;
    let screen_w = screen.width as f64;

    let dock_left = center_x < screen_w / 2.0;
    let peek = (PEEK_WIDTH * scale) as i32;
    let already_docked = pos.x <= 0 || (pos.x as f64 + win_w) >= screen_w - 2.0;

    let new_x = if already_docked {
        if dock_left {
            (12.0 * scale) as i32
        } else {
            (screen_w - win_w - 12.0 * scale) as i32
        }
    } else {
        if dock_left {
            -(win_w as i32) + peek
        } else {
            (screen_w - peek as f64) as i32
        }
    };

    let _ = window.set_position(PhysicalPosition::new(new_x, pos.y));
}

/// 保存窗口位置与大小（节流 400ms，关闭时强制保存）
/// 注意：隐藏中的窗口（横幅模式）不能保存——Windows 给隐藏/最小化窗口
/// 返回 -32000 占位坐标，存进去后下次启动窗口会被"恢复"到屏幕外。
fn save_win_state(window: &WebviewWindow) {
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) else { return };
    if pos.x <= -30000 || pos.y <= -30000 {
        return; // Windows 隐藏窗口占位坐标
    }
    let db = window.app_handle().state::<storage::Db>();
    let json = format!(
        "{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}",
        pos.x, pos.y, size.width, size.height
    );
    let _ = storage::kv_set(&db, "win_state", &json);
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 把窗口位置夹回工作区可视范围（防止恢复到屏外/收起位导致"窗口消失"）
fn clamp_to_workarea(window: &WebviewWindow) {
    let mon = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(mon) = mon else { return };
    let wa = mon.work_area();
    let pos = window.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
    let size = window.outer_size().unwrap_or(PhysicalSize::new(360, 640));

    let min_vis = 120; // 至少露出这么多像素可供拖拽
    let mut x = pos.x;
    let mut y = pos.y;

    // 离谱值（-32000 占位坐标等）直接回右上安全位
    if pos.x <= -30000 || pos.y <= -30000 {
        x = wa.position.x + wa.size.width as i32 - size.width as i32 - 24;
        y = wa.position.y + 80;
    } else {
        let max_x = wa.position.x + wa.size.width as i32 - min_vis;
        let min_x = wa.position.x - size.width as i32 + min_vis;
        if x > max_x { x = max_x; }
        if x < min_x { x = wa.position.x + 24; }
        let max_y = wa.position.y + wa.size.height as i32 - min_vis;
        if y > max_y { y = max_y; }
        if y < wa.position.y { y = wa.position.y; }
    }

    if x != pos.x || y != pos.y {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// 启动时恢复上次的窗口位置与大小
fn restore_win_state(window: &WebviewWindow, db: &storage::Db) {
    let Ok(Some(s)) = storage::kv_get(db, "win_state") else { return };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else { return };
    if let (Some(w), Some(h)) = (v["w"].as_u64(), v["h"].as_u64()) {
        if w >= 200 && h >= 300 {
            let _ = window.set_size(PhysicalSize::new(w as u32, h as u32));
        }
    }
    if let (Some(x), Some(y)) = (v["x"].as_i64(), v["y"].as_i64()) {
        let _ = window.set_position(PhysicalPosition::new(x as i32, y as i32));
    }
}

/// 拖拽结束时自动吸附检测
fn snap_to_edge(window: &WebviewWindow) {
    let monitor = match window.current_monitor() {
        Ok(Some(m)) => m,
        _ => return,
    };
    let screen = monitor.size();
    let pos = window.outer_position().unwrap_or(PhysicalPosition::new(0, 0));
    let size = window.outer_size().unwrap_or(PhysicalSize::new(360, 640));

    let screen_w = screen.width as f64;
    let screen_h = screen.height as f64;
    let win_w = size.width as f64;
    let win_h = size.height as f64;

    let mut x = pos.x as f64;
    let mut y = pos.y as f64;

    if x < EDGE_THRESHOLD {
        x = 0.0;
    } else if (screen_w - (x + win_w)) < EDGE_THRESHOLD {
        x = screen_w - win_w;
    }

    if y < 0.0 {
        y = 0.0;
    } else if y + win_h > screen_h {
        y = screen_h - win_h;
    }

    let _ = window.set_position(PhysicalPosition::new(x as i32, y as i32));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            set_always_on_top,
            minimize_window,
            show_band,
            resize_band,
            restore_main,
            hide_to_tray,
            quit_app,
            ask_ai,
            analyze_stock,
            explain_sentiment,
            toggle_dock_edge,
            fetch_quotes,
            search_stocks,
            list_sources,
            fetch_news,
            fetch_stock_news,
            fetch_sentiment,
            fetch_kline,
            ai_recommend,
            db_get,
            db_set
        ])
        .setup(|app| {
            // SQLite：放 app data 目录，随系统用户走
            let data_dir = app.path().app_data_dir().expect("无法获取 app data 目录");
            let db = storage::init_db(data_dir).expect("初始化 SQLite 失败");

            // 系统托盘：左键点图标还原主窗，右键弹菜单（显示/退出）
            let show_i = MenuItem::with_id(app, "show", "显示主窗", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出 rust-stock", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_i, &quit_i])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("rust-stock")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            let win = app.get_webview_window("main").unwrap();
            let _ = win.set_always_on_top(true);

            // 恢复上次窗口位置/大小（在 manage 之前用本地引用读），并夹回可视区
            restore_win_state(&win, &db);
            clamp_to_workarea(&win);
            app.manage(db);

            // 监听窗口事件：移动后吸附 + 节流保存位置大小，关闭时强制保存
            use std::sync::atomic::{AtomicU64, Ordering};
            static LAST_SAVE: AtomicU64 = AtomicU64::new(0);
            let win_clone = win.clone();
            win.on_window_event(move |event| match event {
                tauri::WindowEvent::Moved(_) => {
                    snap_to_edge(&win_clone);
                    let now = now_ms();
                    if now.saturating_sub(LAST_SAVE.load(Ordering::Relaxed)) > 400 {
                        LAST_SAVE.store(now, Ordering::Relaxed);
                        save_win_state(&win_clone);
                    }
                }
                tauri::WindowEvent::Resized(_) => {
                    let now = now_ms();
                    if now.saturating_sub(LAST_SAVE.load(Ordering::Relaxed)) > 400 {
                        LAST_SAVE.store(now, Ordering::Relaxed);
                        save_win_state(&win_clone);
                    }
                }
                tauri::WindowEvent::CloseRequested { .. } => {
                    save_win_state(&win_clone);
                }
                _ => {}
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running rust-stock");
}

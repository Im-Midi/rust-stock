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
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

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

/// 流式聊天：逐 delta emit "ai-chunk"，结束 "ai-done"，错误 "ai-error"
#[tauri::command]
async fn ask_ai(
    window: WebviewWindow,
    key: String,
    base_url: Option<String>,
    model: Option<String>,
    question: String,
    history: Vec<serde_json::Value>,
) -> Result<(), String> {
    let cfg = ai::AiConfig::new(key, base_url, model)?;
    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": "你是嵌在悬浮行情助手 rust-stock 里的股票 AI。回答简洁（手机宽度的窗口），中文，涉及操作建议时务必提示风险、注明仅供参考。"
    })];
    messages.extend(history);
    messages.push(serde_json::json!({ "role": "user", "content": question }));

    let result = ai::chat_stream(&cfg, messages, 0.6, |delta| {
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
        "对A股股票「{name}」（代码 {code}，现价 {price:.2}，今日涨跌 {change_pct:+.2}%）\
         给出短线综合看涨/看跌判断。严格只输出 JSON：\
         {{\"score\": -100到100的整数（越看涨越接近100，越看跌越接近-100，中性为0）, \
         \"analysis\": \"200字以内的分析理由，说明为什么是这个分数\"}}"
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
        "{context}。你是严谨的A股分析师，请经过详尽的基本面、技术面、消息面综合分析后，\
         推荐 3 支未来一周值得关注的A股股票（避开 ST、退市风险股）。\
         严格只输出 JSON 数组，共 3 个元素：\
         [{{\"code\":\"sh600519 或 sz000001 这种格式\",\"name\":\"股票名称\",\
         \"score\":-100到100的整数（看涨强度）,\
         \"reason\":\"不少于150字的详尽分析：基本面/技术面/消息面依据 + 风险提示\"}}]"
    );
    let messages = vec![
        serde_json::json!({ "role": "system", "content": "你是严谨的股票分析助手。只输出 JSON 数组，不输出任何其他文字。推荐仅供参考，不构成投资建议。" }),
        serde_json::json!({ "role": "user", "content": prompt }),
    ];
    let content = ai::chat_once(&cfg, messages, 0.5).await?;
    let arr = ai::extract_json_array(&content)?;
    let list = arr.as_array().ok_or("AI 返回的不是数组")?;
    let mut out = Vec::new();
    for item in list.iter().take(3) {
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
        out.push(RecStock {
            code,
            name: item["name"].as_str().unwrap_or("").to_string(),
            score: item["score"].as_i64().unwrap_or(0).clamp(-100, 100) as i32,
            reason: item["reason"].as_str().unwrap_or("").to_string(),
        });
    }
    if out.is_empty() {
        return Err("AI 没有返回合法的推荐结果，请重试".into());
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
fn save_win_state(window: &WebviewWindow) {
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) else { return };
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
            ask_ai,
            analyze_stock,
            explain_sentiment,
            toggle_dock_edge,
            fetch_quotes,
            list_sources,
            fetch_news,
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

            let win = app.get_webview_window("main").unwrap();
            let _ = win.set_always_on_top(true);

            // 恢复上次窗口位置/大小（在 manage 之前用本地引用读）
            restore_win_state(&win, &db);
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

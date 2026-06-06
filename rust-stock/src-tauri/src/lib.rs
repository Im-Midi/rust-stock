// rust-stock — Tauri 本地逻辑层（无服务器，全部本地运行）
// 核心：手机尺寸窗口、无边框、置顶、拖到屏幕边缘自动吸附收起 + 本地行情抓取

mod feed;
mod quote;
mod storage;
use quote::Quote;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

const EDGE_THRESHOLD: f64 = 24.0; // 距屏幕边缘多少像素触发吸附
const PEEK_WIDTH: f64 = 6.0;       // 收起后露出的小条宽度

// ---------- 抓取行情（本地直连公开接口，可切换数据源）----------
// source: "sina" | "eastmoney"
// codes:  sina 用 ["sh000001","sz399001"]; eastmoney 用 ["1.000001","0.399001"]
#[tauri::command]
async fn fetch_quotes(source: String, codes: Vec<String>) -> Result<Vec<Quote>, String> {
    let refs: Vec<&str> = codes.iter().map(|s| s.as_str()).collect();
    match source.as_str() {
        "eastmoney" => quote::fetch_eastmoney(&refs).await,
        _ => quote::fetch_sina(&refs).await, // 默认新浪
    }
}

// ---------- 置顶切换 ----------
#[tauri::command]
fn set_always_on_top(window: WebviewWindow, pinned: bool) {
    let _ = window.set_always_on_top(pinned);
}

// ---------- 最小化 ----------
#[tauri::command]
fn minimize_window(window: WebviewWindow) {
    let _ = window.minimize();
}

// ---------- 本地存储（SQLite KV，替代 localStorage）----------
#[tauri::command]
fn db_get(db: tauri::State<storage::Db>, key: String) -> Result<Option<String>, String> {
    storage::kv_get(&db, &key)
}

#[tauri::command]
fn db_set(db: tauri::State<storage::Db>, key: String, value: String) -> Result<(), String> {
    storage::kv_set(&db, &key, &value)
}

// ---------- 真实快讯 / 市场情绪 ----------
#[tauri::command]
async fn fetch_news() -> Result<Vec<feed::NewsItem>, String> {
    feed::fetch_news().await
}

#[tauri::command]
async fn fetch_sentiment() -> Result<feed::Sentiment, String> {
    feed::fetch_sentiment().await
}

// ---------- AI 解读市场情绪（点击表盘翻面时调用）----------
#[tauri::command]
async fn explain_sentiment(
    key: String,
    score: f64,
    label: String,
    detail: String, // 前端拼好的指数明细，如 "上证指数 -2.10%，深证成指 -1.80%…"
) -> Result<String, String> {
    if key.trim().is_empty() {
        return Err("未配置 DeepSeek API Key".into());
    }
    let prompt = format!(
        "当前A股市场情绪指标 {score:.1}（区间 -100 极度恐慌 ~ +100 极度乐观），档位「{label}」。\
         指标由主要指数涨跌幅加权得出：{detail}。\
         请用150字以内中文解释为什么市场情绪处于这个档位：结合各指数表现，推测可能的宏观/资金面因素。\
         直接输出正文（不要 markdown），结尾注明仅供参考。"
    );
    let body = serde_json::json!({
        "model": "deepseek-chat",
        "messages": [
            { "role": "system", "content": "你是简洁严谨的市场分析助手。" },
            { "role": "user", "content": prompt }
        ],
        "temperature": 0.4
    });
    let resp = reqwest::Client::new()
        .post("https://api.deepseek.com/chat/completions")
        .header("Authorization", format!("Bearer {}", key.trim()))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("请求 DeepSeek 失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("DeepSeek 返回 {status}: {text}"));
    }
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string())
}

// ---------- AI 流式聊天（本地直连 DeepSeek SSE，用户自带 key）----------
// 流程：POST stream:true → 逐行读 SSE → 每个 delta 用事件 "ai-chunk" 发给前端
// 结束（[DONE] 或流关闭）发 "ai-done"；出错发 "ai-error"。
#[tauri::command]
async fn ask_ai(
    window: WebviewWindow,
    key: String,
    question: String,
    history: Vec<serde_json::Value>, // [{role, content}]，前端维护上下文
) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("未配置 DeepSeek API Key".into());
    }
    let mut messages = vec![serde_json::json!({
        "role": "system",
        "content": "你是嵌在悬浮行情助手 rust-stock 里的股票 AI。回答简洁（手机宽度的窗口），中文，涉及操作建议时务必提示风险、注明仅供参考。"
    })];
    messages.extend(history);
    messages.push(serde_json::json!({ "role": "user", "content": question }));

    let body = serde_json::json!({
        "model": "deepseek-chat",
        "messages": messages,
        "stream": true,
        "temperature": 0.6
    });

    let resp = reqwest::Client::new()
        .post("https://api.deepseek.com/chat/completions")
        .header("Authorization", format!("Bearer {}", key.trim()))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("请求 DeepSeek 失败: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let msg = format!("DeepSeek 返回 {status}: {text}");
        let _ = window.emit("ai-error", &msg);
        return Err(msg);
    }

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = window.emit("ai-error", format!("流中断: {e}"));
                break;
            }
        };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        // SSE 按行分割；保留最后一段可能不完整的行
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                let _ = window.emit("ai-done", ());
                return Ok(());
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                    if !delta.is_empty() {
                        let _ = window.emit("ai-chunk", delta);
                    }
                }
            }
        }
    }
    let _ = window.emit("ai-done", ());
    Ok(())
}

// ---------- AI 个股看涨/看跌分析（本地直连 DeepSeek）----------
#[derive(serde::Serialize)]
pub struct AiAnalysis {
    pub score: i32,       // -100(极度看跌) ~ 100(极度看涨)
    pub analysis: String, // 打分理由
}

#[tauri::command]
async fn analyze_stock(
    key: String,
    name: String,
    code: String,
    price: f64,
    change_pct: f64,
) -> Result<AiAnalysis, String> {
    if key.trim().is_empty() {
        return Err("未配置 DeepSeek API Key".into());
    }
    let prompt = format!(
        "对A股股票「{name}」（代码 {code}，现价 {price:.2}，今日涨跌 {change_pct:+.2}%）\
         给出短线综合看涨/看跌判断。严格只输出 JSON：\
         {{\"score\": -100到100的整数（越看涨越接近100，越看跌越接近-100，中性为0）, \
         \"analysis\": \"200字以内的分析理由，说明为什么是这个分数\"}}"
    );
    let body = serde_json::json!({
        "model": "deepseek-chat",
        "messages": [
            { "role": "system", "content": "你是严谨的股票分析助手。只输出 JSON，不输出任何其他文字。分析仅供参考，不构成投资建议。" },
            { "role": "user", "content": prompt }
        ],
        "temperature": 0.3
    });

    let resp = reqwest::Client::new()
        .post("https://api.deepseek.com/chat/completions")
        .header("Authorization", format!("Bearer {}", key.trim()))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("请求 DeepSeek 失败: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("DeepSeek 返回 {status}: {text}"));
    }

    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {e}"))?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("DeepSeek 响应缺少 content")?;

    // 容错：截取第一个 { 到最后一个 }（防止模型包了 markdown 代码块）
    let start = content.find('{').ok_or("AI 未返回 JSON")?;
    let end = content.rfind('}').ok_or("AI 未返回 JSON")?;
    let parsed: serde_json::Value = serde_json::from_str(&content[start..=end])
        .map_err(|e| format!("解析 AI JSON 失败: {e}"))?;

    Ok(AiAnalysis {
        score: parsed["score"].as_i64().unwrap_or(0).clamp(-100, 100) as i32,
        analysis: parsed["analysis"].as_str().unwrap_or("").to_string(),
    })
}

// ---------- 吸附到最近的屏幕边缘 / 展开 ----------
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

    // 判断离左右哪边更近
    let dock_left = center_x < screen_w / 2.0;
    let peek = (PEEK_WIDTH * scale) as i32;

    // 简单的「收起 / 展开」状态：用窗口当前 x 是否已贴边来判断
    let already_docked = pos.x <= 0 || (pos.x as f64 + win_w) >= screen_w - 2.0;

    let new_x = if already_docked {
        // 展开：完整露出，留 12px 边距
        if dock_left { (12.0 * scale) as i32 }
        else { (screen_w - win_w - 12.0 * scale) as i32 }
    } else {
        // 收起：只露出 peek 宽度的小条
        if dock_left { -(win_w as i32) + peek }
        else { (screen_w - peek as f64) as i32 }
    };

    let _ = window.set_position(PhysicalPosition::new(new_x, pos.y));
}

// ---------- 拖拽结束时自动吸附检测 ----------
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

    // 左/右边缘吸附
    if x < EDGE_THRESHOLD { x = 0.0; }
    else if (screen_w - (x + win_w)) < EDGE_THRESHOLD { x = screen_w - win_w; }

    // 上下边界保护，避免拖出屏幕
    if y < 0.0 { y = 0.0; }
    else if y + win_h > screen_h { y = screen_h - win_h; }

    let _ = window.set_position(PhysicalPosition::new(x as i32, y as i32));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            set_always_on_top,
            minimize_window,
            ask_ai,
            analyze_stock,
            toggle_dock_edge,
            fetch_quotes,
            fetch_news,
            fetch_sentiment,
            explain_sentiment,
            db_get,
            db_set
        ])
        .setup(|app| {
            // SQLite：放 app data 目录，随系统用户走
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("无法获取 app data 目录");
            let db = storage::init_db(data_dir).expect("初始化 SQLite 失败");
            app.manage(db);

            let win = app.get_webview_window("main").unwrap();
            // 启动即置顶
            let _ = win.set_always_on_top(true);

            // 监听窗口移动，松手后吸附
            let win_clone = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::Moved(_) = event {
                    // Moved 在拖拽过程中持续触发；用防抖在实际工程中更佳。
                    // 这里简单地在每次移动后做边缘检测。
                    snap_to_edge(&win_clone);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running rust-stock");
}

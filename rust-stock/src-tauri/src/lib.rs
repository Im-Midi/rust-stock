// rust-stock — Tauri 本地逻辑层（无服务器，全部本地运行）
// 架构：
//   sources/  行情数据源抽象（QuoteSource trait + 注册表，新增源零改动）
//   ai.rs     AI Provider 抽象（OpenAI 兼容协议，base_url/model 可配，默认 DeepSeek）
//   feed.rs   快讯 + 市场情绪算法
//   storage.rs SQLite KV 持久化
//   quote.rs  行情数据模型与解析器（含单测）

mod ai;
mod extra;
mod feed;
mod kline;
mod quote;
mod sources;
mod storage;

use quote::Quote;
use tauri::{Emitter, Manager, WebviewWindow};
#[cfg(desktop)]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    PhysicalPosition, PhysicalSize,
};

#[cfg(desktop)]
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

#[cfg(desktop)]
const EDGE_THRESHOLD: f64 = 24.0; // 距屏幕边缘多少像素触发吸附
#[cfg(desktop)]
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

/// 板块热力（行业板块涨跌幅，真实数据）
#[tauri::command]
async fn fetch_sectors() -> Result<Vec<extra::Sector>, String> {
    extra::fetch_sectors().await
}

/// 个股资金流（主力/超大/大/中/小单净额）
#[tauri::command]
async fn fetch_fund_flow(code: String) -> Result<extra::FundFlow, String> {
    extra::fetch_fund_flow(&code).await
}

/// 今日推荐候选池（涨幅榜+主力净流入榜+龙虎榜合并，前端可预览数量）
#[tauri::command]
async fn fetch_candidates() -> Result<Vec<extra::Candidate>, String> {
    extra::fetch_candidates().await
}

/// 历史主力资金流（近 N 个交易日，东财 fflow daykline，升序旧→新）
#[tauri::command]
async fn fetch_flow_history(code: String, days: u32) -> Result<Vec<extra::FlowDay>, String> {
    extra::fetch_flow_history(&code, days).await
}

/// 个股所属板块/概念（东财 F10，行业排最前，已滤指数成分类噪音）
#[tauri::command]
async fn fetch_stock_boards(code: String) -> Result<Vec<extra::StockBoard>, String> {
    extra::fetch_stock_boards(&code).await
}

/// 北向资金（沪深港通）近 5 个交易日成交金额（亿元）。
/// 净买额自 2024-08 起交易所不再披露（实时接口恒 0、历史字段为 null），
/// 仍披露的只有按日成交金额——前端如实标注口径，绝不伪造净流入。
#[tauri::command]
async fn fetch_north_flow() -> Result<Vec<extra::NorthDay>, String> {
    extra::fetch_north_flow().await
}

/// 同花顺 A 股人气榜（小时榜 Top15，普通 GET，code+name+涨跌幅一次到位）
#[tauri::command]
async fn fetch_hot_stocks() -> Result<Vec<extra::HotStock>, String> {
    extra::fetch_hot_stocks().await
}

/// 个股分红历史（最近 8 期，最新方案在前）。Ok(空数组)=该股从未分红
#[tauri::command]
async fn fetch_dividends(code: String) -> Result<Vec<extra::Dividend>, String> {
    extra::fetch_dividends(&code).await
}

/// 个股股本/市值快照（总股本/流通股/总市值/流通市值 + 动态市盈率/市净率）
#[tauri::command]
async fn fetch_share_info(code: String) -> Result<extra::ShareInfo, String> {
    extra::fetch_share_info(&code).await
}

/// 核心财务指标（东财 F10 主要指标，最新报告期一行：营收/净利+同比、EPS、ROE、毛利率、负债率）
#[tauri::command]
async fn fetch_financials(code: String) -> Result<extra::Financials, String> {
    extra::fetch_financials(&code).await
}

/// 融资融券个股余额（最近 2 个交易日，最新在前）。Ok(空数组)=非两融标的
#[tauri::command]
async fn fetch_margin(code: String) -> Result<Vec<extra::MarginDay>, String> {
    extra::fetch_margin(&code).await
}

/// 个股龙虎榜上榜明细（最近 5 次，最新在前）。Ok(空数组)=从未上榜
#[tauri::command]
async fn fetch_lhb_detail(code: String) -> Result<Vec<extra::LhbItem>, String> {
    extra::fetch_lhb_detail(&code).await
}

/// 自选股相关快讯（行情页"自选股信息"卡片）
#[tauri::command]
async fn fetch_stock_news(codes: Vec<String>) -> Result<Vec<feed::NewsItem>, String> {
    feed::fetch_stock_news(&codes).await
}

/// 批量判断新闻标题对相关公司是利好/利空/中性（一次请求判一批，省 token）。
/// 返回与输入等长的数组：1=利好 -1=利空 0=中性。
#[tauri::command]
async fn classify_news(
    key: String,
    base_url: Option<String>,
    model: Option<String>,
    titles: Vec<String>,
) -> Result<Vec<i32>, String> {
    if titles.is_empty() {
        return Ok(vec![]);
    }
    let cfg = ai::AiConfig::new(key, base_url, model)?;
    let numbered: String = titles
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "判断下列每条A股新闻标题对相关公司股价是利好、利空还是中性。         只输出一个 JSON 整数数组，长度必须等于条数（{n} 条），按顺序对应：         1=利好，-1=利空，0=中性或无法判断。不要输出任何解释。\n\n{numbered}",
        n = titles.len()
    );
    let messages = vec![
        serde_json::json!({ "role": "system", "content": "你是A股消息面分析助手，只输出 JSON 整数数组。" }),
        serde_json::json!({ "role": "user", "content": prompt }),
    ];
    let content = ai::chat_once(&cfg, messages, 0.2).await?;
    let arr = ai::extract_json_array(&content)?;
    let list = arr.as_array().ok_or("AI 返回的不是数组")?;
    // 对齐长度：缺失补 0，越界忽略，值钳到 -1/0/1
    let mut out = vec![0i32; titles.len()];
    for (i, v) in list.iter().enumerate() {
        if i >= out.len() {
            break;
        }
        out[i] = match v.as_i64().unwrap_or(0) {
            x if x > 0 => 1,
            x if x < 0 => -1,
            _ => 0,
        };
    }
    Ok(out)
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
5) 给出优先研究清单（3~7家），每家按「卡住的环节/产业链位置（上游是什么、下游卖给谁）/排序原因（需求传导到哪条财务线）/证据方向/主要风险/证伪条件」展开；\
6) A股证据怎么查（核实清单要具体到这些渠道）：定期报告（年报/半年报/季报）、临时公告、交易所问询函与监管函、互动易/上证e互动、招投标与中标公告、客户验厂与认证、环评/能评/地方项目备案、专利与行业协会数据、海关进出口数据、上下游上市公司交叉验证；财务上重点看：应收账款/存货/合同负债/经营现金流、毛利率/产能利用率/在建工程转固、关联交易/资产注入真实性、定增/可转债/股权质押/商誉、补贴与订单的商业性、主题炒作后的估值压力；\
7) 给每家候选一个 0~100 的研究优先级倾向分，并说明权重构成：需求拐点、卡点严重度、供应商集中度、扩产难度、证据质量、估值背离、催化时点为加分项；稀释融资、治理瑕疵、地缘风险、流动性差、纯炒作、会计质量、强周期为减分项（85+顶级优先/70+高优先/55+值得跟踪/55以下早期或低优先）；\
8) 指出一个市场热捧但你排序靠后的方向并解释为什么；\
9) 结尾给「下一步核实清单」，对应到第6步的具体渠道。\
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
    context: Option<String>, // 数据注入层：前端算好的真实数据(筹码/指标/行情)
    mode: Option<String>,    // None/缺省=产业链基本面分析；"technical"=盘面技术解读
) -> Result<AiAnalysis, String> {
    let cfg = ai::AiConfig::new(key, base_url, model)?;
    let prompt = if mode.as_deref() == Some("technical") {
        // 盘面技术解读：聚焦注入的筹码/指标/量价，与自选股的产业链打分区分开
        format!("结合上文提供的真实数据（筹码分布 / 通达信指标 / 行情），对A股「{name}」（代码 {code}）做一份**盘面技术解读**（不要写产业链/基本面长篇）。严格只输出 JSON：{{\"score\": -100到100的整数（越看多越接近100，越看空越接近-100，中性0）, \"analysis\": \"350~500字，依次覆盖并用「」标出：「筹码结构」平均成本与现价的关系、获利盘/套牢盘的抛压或支撑、成本密集区作为支撑/压力位；「技术信号」MACD/KDJ/RSI/BOLL 的金叉死叉、超买超卖与可能的顶底背离及其含义；「量价与位置」当前处于突破/回踩/震荡中的哪一种，标出关键支撑位与压力位（可用成本区间与 BOLL 上下轨）；「短线研判」综合上述给出方向与力度；「失效信号」跌破或站上哪个价位说明该研判失效。除上文已给的数字外不得编造其他具体数值。\"}}")
    } else {
        format!(
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
        )
    };
    let mut messages = vec![
        serde_json::json!({ "role": "system", "content": "你是严谨的股票分析助手。只输出 JSON，不输出任何其他文字。分析仅供参考，不构成投资建议。" }),
    ];
    // 数据注入层（伪对话）：把前端算好的真实数据当作"模型已查到的事实"喂入，
    // 系统提示与分析模板一字不动，只补数据上下文（go-stock 伪对话注入思路）。
    if let Some(ctx) = context.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        messages.push(serde_json::json!({ "role": "user", "content": format!("请先提供「{name}」({code})当前可用的实时数据") }));
        messages.push(serde_json::json!({ "role": "assistant", "content": ctx }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": format!("{prompt}\n\n【强制规则】必须基于上文提供的实时数据作答，涉及具体数字一律以上文为准；上文未提供的数据不得凭记忆编造，须注明「未获取到」。") }));
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

/// 单股产业链深度调研（复用「研」的方法论 RESEARCH_SYSTEM）。
/// 失败返回空串——单股深调失败只是该股缺「深度调研」段，不拖垮整次推荐。
async fn deep_research_stock(
    cfg: &ai::AiConfig,
    name: &str,
    code: &str,
    price: f64,
    change_pct: f64,
    extra: &str,
) -> String {
    let real = if price > 0.0 {
        format!("真实行情：现价 {price:.2}，今日涨跌 {change_pct:+.2}%{extra}（这些数字真实，可引用）")
    } else {
        "（暂无实时行情，禁止编造价格/涨跌幅）".to_string()
    };
    let user = format!("请对A股「{name}」（代码 {code}）做单股产业链深度调研。{real}\n套用你的产业链调研方法，分节输出、每节用「」标出，中文 400~600 字：\n「产业链位置」在哪条产业链的哪一层，上游供给什么、下游卖给谁；\n「卡点与稀缺性」控制/供应稀缺环节还是仅受益主题，稀缺来自认证周期/扩产难度/工艺壁垒哪项；\n「多流派多空」价值/成长/游资/技术/宏观 各一句给倾向；\n「催化与验证」未来1~4个季度看哪些财报/公告/订单信号确认或推翻；\n「主要风险」与「证伪条件」各一节；\n「研究优先级」给 0~100 倾向分并说明加减分。\n除给定的现价与涨跌幅外禁止编造其他具体数字。直接输出正文，不要 markdown 代码块。");
    let messages = vec![
        serde_json::json!({ "role": "system", "content": RESEARCH_SYSTEM }),
        serde_json::json!({ "role": "user", "content": user }),
    ];
    ai::chat_once(cfg, messages, 0.4).await.unwrap_or_default()
}

#[tauri::command]
async fn ai_recommend(
    key: String,
    base_url: Option<String>,
    model: Option<String>,
    context: String, // 盘面背景（日期/情绪/指数）
) -> Result<Vec<RecStock>, String> {
    let cfg = ai::AiConfig::new(key, base_url, model)?;

    // 1) 本地从真实数据筛候选池（涨幅榜+主力净流入榜+龙虎榜合并），失败则用纯 AI 兜底
    let candidates = extra::fetch_candidates().await.unwrap_or_default();
    let pool_text = if candidates.is_empty() {
        String::from("（实时候选池获取失败，请基于你的知识谨慎给出候选，并在每条注明需人工核实行情）")
    } else {
        let mut lines = String::from("以下是本地用真实行情筛出的候选池（这些数字是真实的，可直接引用，禁止改写）：\n");
        for c in candidates.iter().take(60) {
            lines.push_str(&format!(
                "{} {} 现价{:.2} 涨跌{:+.2}% 换手{:.1}% 主力净额{:.0}万{}\n",
                c.code, c.name, c.price, c.change_pct, c.turnover, c.main_flow / 1e4,
                if c.on_lhb { " [今日上龙虎榜]" } else { "" }
            ));
        }
        lines
    };

    // 2) 多流派 + 供应链 + 龙虎榜 的深度 prompt（ai-hedge-fund 范式）
    let prompt = format!(
        "{context}。\n{pool_text}\n         请从上述候选池里精选 6~8 支今日最值得关注的A股，按以下方法综合判断：\n         A) 供应链瓶颈研究法打底：判断它在产业链哪一层、是否卡住稀缺环节、上下游传导；\n         B) 多流派视角各自打分（借鉴 ai-hedge-fund）：用 价值派(护城河/估值安全边际)、成长派(行业S曲线/TAM)、游资打板派(题材热度/龙虎榜/换手量能)、技术派(趋势/突破/均线)、宏观派(政策/资金面) 五个视角分别给倾向，再综合裁决；\n         C) 龙虎榜与主力资金：标注[今日上龙虎榜]或主力大幅净流入的，是游资/机构关注信号，要重点说明；\n         重要：当日下跌不是淘汰理由——回调中的优质股(基本面强、卡住稀缺环节、主力仍在流入)同样可以推荐，请说明逻辑；避开 ST/退市风险股。\n         每支 reason 写 200~350 字，依次覆盖：「产业链位置」「多流派分歧(逐个流派一句话:价值/成长/游资/技术/宏观)」「龙虎榜/资金信号」「为什么今日值得关注」「主要风险」「证伪条件」。\n         禁止编造候选池里没有的价格/涨跌幅。严格只输出 JSON 数组，每元素：\n         [{{\"code\":\"sh600519\",\"name\":\"名称\",\"score\":1到100整数(综合看好度),\"reason\":\"…\"}}]"
    );
    let messages = vec![
        serde_json::json!({ "role": "system", "content": "你是严谨的A股产业链+多流派研究助手，方法论：供应链瓶颈优先，叠加价值/成长/游资/技术/宏观多视角打分后综合裁决。只输出 JSON 数组，不输出任何其他文字。推荐仅供参考，不构成投资建议。" }),
        serde_json::json!({ "role": "user", "content": prompt }),
    ];
    let content = ai::chat_once(&cfg, messages, 0.5).await?;
    let arr = ai::extract_json_array(&content)?;
    let list = arr.as_array().ok_or("AI 返回的不是数组")?;

    // 3) 候选池建索引（真实行情回填，AI 不编数字）
    use std::collections::HashMap;
    let by_code: HashMap<&str, &extra::Candidate> =
        candidates.iter().map(|c| (c.code.as_str(), c)).collect();

    let mut out: Vec<RecStock> = Vec::new();
    for item in list.iter().take(8) {
        let raw_code = item["code"].as_str().unwrap_or("").trim().to_lowercase();
        let code = if raw_code.len() == 6 && raw_code.chars().all(|c| c.is_ascii_digit()) {
            let p = if raw_code.starts_with('6') || raw_code.starts_with('5') || raw_code.starts_with('9') { "sh" } else { "sz" };
            format!("{p}{raw_code}")
        } else {
            raw_code
        };
        if !(code.len() == 8 && (code.starts_with("sh") || code.starts_with("sz"))) {
            continue;
        }
        let score = item["score"].as_i64().unwrap_or(0);
        if score <= 0 {
            continue;
        }
        // 用候选池真实行情回填（不淘汰跌幅！）；候选池里没有的，price=0 标识未复核
        let (price, change_pct, name) = match by_code.get(code.as_str()) {
            Some(c) => (c.price, c.change_pct, c.name.clone()),
            None => (0.0, 0.0, item["name"].as_str().unwrap_or("").to_string()),
        };
        out.push(RecStock {
            code,
            name: if name.is_empty() { item["name"].as_str().unwrap_or("").to_string() } else { name },
            score: score.clamp(1, 100) as i32,
            reason: item["reason"].as_str().unwrap_or("").to_string(),
            price,
            change_pct,
        });
    }
    out.sort_by(|a, b| b.score.cmp(&a.score));
    if out.is_empty() {
        return Err("AI 未返回有效推荐，请点「重新生成」重试".into());
    }
    out.truncate(8); // 仅对前 8 支做逐股深度调研，控制耗时与 Token

    // 阶段2：对每支入选股再做单股产业链深度调研（与「研」同源），并发执行。
    {
        use tokio::task::JoinSet;
        let mut set: JoinSet<(usize, String)> = JoinSet::new();
        for (i, r) in out.iter().enumerate() {
            let cfg2 = cfg.clone();
            let name = r.name.clone();
            let code = r.code.clone();
            let price = r.price;
            let chg = r.change_pct;
            let extra = by_code
                .get(code.as_str())
                .map(|c| format!("，换手 {:.1}%，主力净额 {:.0}万{}", c.turnover, c.main_flow / 1e4, if c.on_lhb { "，今日上龙虎榜" } else { "" }))
                .unwrap_or_default();
            set.spawn(async move {
                (i, deep_research_stock(&cfg2, &name, &code, price, chg, &extra).await)
            });
        }
        while let Some(res) = set.join_next().await {
            if let Ok((i, deep)) = res {
                if !deep.trim().is_empty() {
                    out[i].reason = format!("{}\n\n──────── 深度调研 ────────\n{}", out[i].reason, deep);
                }
            }
        }
    }

    Ok(out)
}

// ============================================================
// 窗口控制
// ============================================================

#[tauri::command]
fn set_always_on_top(window: WebviewWindow, pinned: bool) {
    #[cfg(desktop)]
    let _ = window.set_always_on_top(pinned);
    #[cfg(not(desktop))]
    let _ = (window, pinned);
}

#[tauri::command]
fn minimize_window(window: WebviewWindow) {
    #[cfg(desktop)]
    let _ = window.minimize();
    #[cfg(not(desktop))]
    let _ = window;
}

/// 最小化到系统托盘：藏主窗（托盘图标常驻，点图标/菜单可还原）
#[tauri::command]
fn hide_to_tray(window: WebviewWindow) {
    #[cfg(desktop)]
    {
        let _ = window.hide();
        if let Some(band) = window.app_handle().get_webview_window("band") {
            let _ = band.hide();
        }
    }
    #[cfg(not(desktop))]
    let _ = window;
}

/// 彻底退出应用
#[tauri::command]
fn quit_app(window: WebviewWindow) {
    #[cfg(desktop)]
    {
        save_win_state(&window); // 退出前存一次窗口位置/大小
        window.app_handle().exit(0);
    }
    #[cfg(not(desktop))]
    let _ = window;
}

#[cfg(desktop)]
const BAND_W: f64 = 86.0; // 挂件逻辑宽度

/// 缩小为屏幕右缘的竖排仪表盘挂件：隐藏主窗，band 窗贴右边显示。
/// 挂件顶部有拖把手，用户可自行拖到任意位置。
#[tauri::command]
fn show_band(window: WebviewWindow) {
    #[cfg(not(desktop))]
    { let _ = window; return; }
    #[cfg(desktop)]
    {
    let app = window.app_handle();
    let Some(band) = app.get_webview_window("band") else { return };
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
}

/// 挂件按自选数量自适应高度（band 前端渲染完调用，逻辑像素）
#[tauri::command]
fn resize_band(window: WebviewWindow, height: f64) {
    #[cfg(not(desktop))]
    { let _ = (window, height); return; }
    #[cfg(desktop)]
    {
    let Ok(Some(mon)) = window.current_monitor() else { return };
    let scale = mon.scale_factor();
    let w = (BAND_W * scale) as u32;
    let h = (height.clamp(70.0, 720.0) * scale) as u32;
    let _ = window.set_size(PhysicalSize::new(w, h));
    }
}

/// 从横幅点击还原主窗
#[tauri::command]
fn restore_main(window: WebviewWindow) {
    #[cfg(not(desktop))]
    { let _ = window; return; }
    #[cfg(desktop)]
    {
    let app = window.app_handle();
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        clamp_to_workarea(&main);
        let _ = main.set_focus();
    }
    let _ = window.hide();
    }
}

/// 吸附到最近的屏幕边缘 / 展开
#[tauri::command]
fn toggle_dock_edge(window: WebviewWindow) {
    #[cfg(not(desktop))]
    { let _ = window; return; }
    #[cfg(desktop)]
    {
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
}

/// 保存窗口位置与大小（节流 400ms，关闭时强制保存）
/// 注意：隐藏中的窗口（横幅模式）不能保存——Windows 给隐藏/最小化窗口
/// 返回 -32000 占位坐标，存进去后下次启动窗口会被"恢复"到屏幕外。
#[cfg(desktop)]
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

#[cfg(desktop)]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 把窗口位置夹回工作区可视范围（防止恢复到屏外/收起位导致"窗口消失"）
#[cfg(desktop)]
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
#[cfg(desktop)]
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
#[cfg(desktop)]
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
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        None,
    ));
    builder
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
            classify_news,
            fetch_sectors,
            fetch_fund_flow,
            fetch_candidates,
            fetch_flow_history,
            fetch_stock_boards,
            fetch_north_flow,
            fetch_hot_stocks,
            fetch_dividends,
            fetch_share_info,
            fetch_financials,
            fetch_margin,
            fetch_lhb_detail,
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

            #[cfg(not(desktop))]
            app.manage(db);

            #[cfg(desktop)]
            {
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
            } // end #[cfg(desktop)] setup block

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running rust-stock");
}

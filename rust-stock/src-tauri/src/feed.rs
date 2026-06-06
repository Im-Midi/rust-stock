// feed.rs — 真实快讯 + 市场情绪
//
// 快讯：东方财富 7×24 全球直播（UTF-8 JSON，公开接口，无需签名）
//   https://np-weblist.eastmoney.com/comm/web/getFastNewsList?client=web&biz=web_724&fastColumn=102&sortEnd=&pageSize=20&req_trace=...
//   （财联社电报需要接口签名/浏览器抓取，go-stock 用 Edge 抓；这里选东财 7×24 作为可直连的真实源）
//
// 情绪：基于真实行情的市场宽度算法 ——
//   用主要指数涨跌幅加权（上证/深成/创业板/沪深300），映射到 -100..100。
//   日内 ±2% 即视为极端（A股常态波动），tanh 软压缩避免顶满。

use serde::{Deserialize, Serialize};

// ============================================================
// 快讯
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub time: String,           // HH:MM
    pub txt: String,            // 标题/摘要
    pub tag: String,            // 栏目标签（可空）
    #[serde(default)]
    pub stocks: Vec<String>,    // 关联标的 secid（东财 stockList，如 "1.601186"）
    #[serde(default)]
    pub url: String,            // 原文链接（个股资讯有，7×24 快讯为空）
}

/// 解析东方财富 7×24 快讯 JSON
/// 返回结构（节选）：
/// {"data":{"fastNewsList":[{"code":"...","showTime":"2026-06-06 11:33:05","title":"...","summary":"..."}]}}
pub fn parse_em_news(body: &str) -> Vec<NewsItem> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let list = match v["data"]["fastNewsList"].as_array() {
        Some(l) => l,
        None => return vec![],
    };
    list.iter()
        .filter_map(|item| {
            // title 优先，没有就用 summary
            let txt = item["title"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or_else(|| item["summary"].as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if txt.is_empty() {
                return None;
            }
            // showTime: "2026-06-06 11:33:05" → "11:33"
            let time = item["showTime"]
                .as_str()
                .and_then(|s| s.split(' ').nth(1))
                .map(|t| t.chars().take(5).collect::<String>())
                .unwrap_or_default();
            // 实测返回没有 column 字段；titleColor==3 是东财标红的要闻
            let tag = item["column"]["name"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    if item["titleColor"].as_i64().unwrap_or(0) == 3 { "要闻".into() } else { String::new() }
                });
            let stocks = item["stockList"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            Some(NewsItem { time, txt, tag, stocks, url: String::new() })
        })
        .collect()
}

/// 解析东财个股资讯（np-listapi getListInfo，2026-06-06 实测）
/// {"code":1,"data":{"list":[{"Art_ShowTime":"2026-06-05 21:19:00","Art_Title":"…","Art_Url":"http://…"}]}}
pub fn parse_stock_news(body: &str, stock_name: &str, secid: &str) -> Vec<NewsItem> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let list = match v["data"]["list"].as_array() {
        Some(l) => l,
        None => return vec![],
    };
    list.iter()
        .filter_map(|a| {
            let txt = a["Art_Title"].as_str()?.trim().to_string();
            if txt.is_empty() {
                return None;
            }
            Some(NewsItem {
                time: a["Art_ShowTime"].as_str().unwrap_or("").to_string(), // 完整 "YYYY-MM-DD HH:MM:SS"，前端按需格式化
                txt,
                tag: stock_name.to_string(), // 用股票名做标签，卡片里能看出属于哪支
                stocks: vec![secid.to_string()],
                url: a["Art_Url"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// 按自选股过滤：快讯的 stockList 命中 secid，或正文包含股票名
pub fn filter_for_stocks(items: Vec<NewsItem>, secids: &[String], names: &[String]) -> Vec<NewsItem> {
    items
        .into_iter()
        .filter(|n| {
            n.stocks.iter().any(|s| secids.iter().any(|x| x == s))
                || names.iter().any(|name| !name.is_empty() && n.txt.contains(name.as_str()))
        })
        .take(20)
        .collect()
}

#[cfg(feature = "net")]
pub async fn fetch_news() -> Result<Vec<NewsItem>, String> {
    fetch_news_n(20).await
}

/// 自选股相关资讯：对每支自选并发拉东财个股新闻（每支 6 条），合并按时间倒序取 20。
/// 任何股票都有历史资讯，这比"过滤实时 7×24 流"覆盖广得多。
#[cfg(feature = "net")]
pub async fn fetch_stock_news(codes: &[String]) -> Result<Vec<NewsItem>, String> {
    if codes.is_empty() {
        return Ok(vec![]);
    }
    // 行情拿股票名做标签；失败不阻塞
    let quotes = match crate::sources::get("eastmoney") {
        Some(src) => src.fetch(codes).await.unwrap_or_default(),
        None => vec![],
    };
    let name_of = |code: &str| {
        quotes
            .iter()
            .find(|q| !q.code.is_empty() && code.ends_with(&q.code))
            .map(|q| q.name.clone())
            .unwrap_or_default()
    };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let client = reqwest::Client::new();

    let mut futs = Vec::new();
    for code in codes.iter().take(10) {
        let Some(secid) = crate::sources::to_secid(code) else { continue };
        let name = name_of(code);
        let url = format!(
            "https://np-listapi.eastmoney.com/comm/web/getListInfo?client=web&mTypeAndCode={secid}&type=1&pageSize=6&req_trace={ts}"
        );
        let client = client.clone();
        futs.push(async move {
            let resp = client
                .get(&url)
                .header("Referer", "https://quote.eastmoney.com/")
                .send()
                .await
                .ok()?;
            let text = resp.text().await.ok()?;
            Some(parse_stock_news(&text, &name, &secid))
        });
    }
    let results = futures_util::future::join_all(futs).await;
    let mut all: Vec<NewsItem> = results.into_iter().flatten().flatten().collect();
    // 按时间倒序，同标题去重（同一篇文章可能同时关联多支自选）
    all.sort_by(|a, b| b.time.cmp(&a.time));
    all.dedup_by(|a, b| a.txt == b.txt);
    all.truncate(20);
    Ok(all)
}

#[cfg(feature = "net")]
async fn fetch_news_n(page_size: u32) -> Result<Vec<NewsItem>, String> {
    // 实测（2026-06-06）：sortEnd 不能为空串、req_trace（毫秒时间戳）必填，缺一个都返回
    // {"code":0,"message":"Required String parameter 'xxx' is not present","data":null}
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let url = format!(
        "https://np-weblist.eastmoney.com/comm/web/getFastNewsList?client=web&biz=web_724&fastColumn=102&sortEnd=0&pageSize={page_size}&req_trace={ts}"
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Referer", "https://kuaixun.eastmoney.com/")
        .send()
        .await
        .map_err(|e| format!("快讯请求失败: {e}"))?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let items = parse_em_news(&text);
    if items.is_empty() {
        return Err("快讯解析为空（接口字段可能变了，把原始返回打出来对一下）".into());
    }
    Ok(items)
}

// ============================================================
// 市场情绪
// ============================================================

/// 单个指数对情绪的贡献（前端翻面页展示用）
#[derive(Debug, Clone, Serialize)]
pub struct SentComp {
    pub name: String,
    pub change_pct: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Sentiment {
    pub score: f64,                // -100..100
    pub label: String,             // 文字档位
    pub components: Vec<SentComp>, // 计算明细
}

/// 由主要指数涨跌幅算情绪分。±2% 日内波动按极端处理，tanh 软压缩。
pub fn calc_sentiment(comps: Vec<SentComp>) -> Sentiment {
    let (mut acc, mut wsum) = (0.0, 0.0);
    for c in &comps {
        acc += c.change_pct * c.weight;
        wsum += c.weight;
    }
    let avg = if wsum > 0.0 { acc / wsum } else { 0.0 };
    // avg ±2% → tanh(1) ≈ ±0.76 → ±76 分；±4% 基本顶满
    let score = (avg / 2.0).tanh() * 100.0;
    let score = (score * 100.0).round() / 100.0;
    let label = match score {
        s if s <= -60.0 => "极度恐慌",
        s if s <= -25.0 => "偏空谨慎",
        s if s < 25.0 => "中性",
        s if s < 60.0 => "偏多乐观",
        _ => "极度乐观",
    }
    .to_string();
    Sentiment { score, label, components: comps }
}

#[cfg(feature = "net")]
pub async fn fetch_sentiment() -> Result<Sentiment, String> {
    // 上证 / 深成 / 创业板 / 沪深300，按市场代表性加权（走数据源注册表，统一代码格式）
    let src = crate::sources::get("eastmoney").ok_or("eastmoney 数据源未注册")?;
    let codes: Vec<String> = ["sh000001", "sz399001", "sz399006", "sh000300"]
        .iter().map(|s| s.to_string()).collect();
    let quotes = src.fetch(&codes).await?;
    if quotes.is_empty() {
        return Err("情绪计算失败：指数行情为空".into());
    }
    let weights = [0.35, 0.25, 0.2, 0.2];
    let comps: Vec<SentComp> = quotes
        .iter()
        .zip(weights.iter())
        .map(|(q, w)| SentComp {
            name: q.name.clone(),
            change_pct: q.change_pct,
            weight: *w,
        })
        .collect();
    Ok(calc_sentiment(comps))
}

// ============================================================
// 单元测试（不依赖网络）
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_em_news() {
        // 按 2026-06-06 实测返回的真实形状：无 column 字段，titleColor==3 为标红要闻
        let raw = r#"{"req_trace":"1749200000000","code":"1","message":"success","data":{"sortEnd":"178","fastNewsList":[
            {"code":"a1","showTime":"2026-06-06 11:44:51","title":"伊朗外长说沟通渠道仍畅通","summary":"【伊朗外长说…】正文","titleColor":0,"stockList":[]},
            {"code":"a2","showTime":"2026-06-06 10:34:38","title":"住房公积金管理条例修订征求意见","summary":"x","titleColor":3,"stockList":["90.BK0451"]},
            {"code":"a3","showTime":"2026-06-06 11:15:00","title":"","summary":"title空时回退summary","titleColor":0},
            {"code":"a4","showTime":"","title":"  ","summary":""}
        ]}}"#;
        let items = parse_em_news(raw);
        assert_eq!(items.len(), 3); // 第四条空文本被过滤
        assert_eq!(items[0].time, "11:44");
        assert_eq!(items[0].txt, "伊朗外长说沟通渠道仍畅通");
        assert_eq!(items[0].tag, ""); // 普通条目无标签
        assert_eq!(items[1].tag, "要闻"); // titleColor==3 标红
        assert_eq!(items[1].stocks, vec!["90.BK0451".to_string()]); // 关联标的被捕获
        assert!(items[0].stocks.is_empty());
        assert_eq!(items[2].txt, "title空时回退summary");
    }

    #[test]
    fn test_parse_stock_news() {
        // 按 2026-06-06 实测返回的真实形状
        let raw = r#"{"code":1,"message":"success","data":{"page_index":1,"list":[
            {"Art_ShowTime":"2026-06-05 21:19:00","Art_Code":"x1","Art_Title":"贵州茅台大宗交易成交0.30万股","Art_Url":"http://finance.eastmoney.com/a/x1.html"},
            {"Art_ShowTime":"2026-06-05 21:03:15","Art_Code":"x2","Art_Title":"","Art_Url":"http://x2"},
            {"Art_ShowTime":"2026-06-06 13:06:08","Art_Code":"x3","Art_Title":"四大消费黄金赛道配置性价比凸显","Art_Url":"http://x3"}
        ]}}"#;
        let items = parse_stock_news(raw, "贵州茅台", "1.600519");
        assert_eq!(items.len(), 2); // 空标题被过滤
        assert_eq!(items[0].tag, "贵州茅台");
        assert_eq!(items[0].stocks, vec!["1.600519".to_string()]);
        assert!(items[0].url.starts_with("http"));
        assert_eq!(items[0].time, "2026-06-05 21:19:00");
        assert_eq!(parse_stock_news("not json", "x", "1.1").len(), 0);
    }

    #[test]
    fn test_filter_for_stocks() {
        let mk = |txt: &str, stocks: Vec<&str>| NewsItem {
            time: "10:00".into(),
            txt: txt.into(),
            tag: String::new(),
            stocks: stocks.into_iter().map(|s| s.to_string()).collect(),
            url: String::new(),
        };
        let items = vec![
            mk("某公司中标大单", vec!["1.601186"]),         // secid 命中
            mk("贵州茅台渠道调研纪要", vec![]),              // 名称命中
            mk("无关的宏观新闻", vec!["0.000002"]),          // 都不命中
        ];
        let secids = vec!["1.601186".to_string()];
        let names = vec!["贵州茅台".to_string()];
        let out = filter_for_stocks(items, &secids, &names);
        assert_eq!(out.len(), 2);
        assert!(out[0].txt.contains("中标"));
        assert!(out[1].txt.contains("茅台"));
    }

    #[test]
    fn test_parse_em_news_garbage() {
        assert_eq!(parse_em_news("not json").len(), 0);
        assert_eq!(parse_em_news(r#"{"data":{}}"#).len(), 0);
    }

    fn comp(pct: f64, w: f64) -> SentComp {
        SentComp { name: "测试指数".into(), change_pct: pct, weight: w }
    }

    #[test]
    fn test_sentiment_neutral() {
        let s = calc_sentiment(vec![comp(0.0, 0.35), comp(0.0, 0.25), comp(0.0, 0.2), comp(0.0, 0.2)]);
        assert_eq!(s.score, 0.0);
        assert_eq!(s.label, "中性");
        assert_eq!(s.components.len(), 4); // 明细原样带回
    }

    #[test]
    fn test_sentiment_bull_bear_symmetry() {
        let bull = calc_sentiment(vec![comp(1.5, 0.5), comp(2.5, 0.5)]);
        let bear = calc_sentiment(vec![comp(-1.5, 0.5), comp(-2.5, 0.5)]);
        assert!(bull.score > 50.0, "大涨应显著偏多: {}", bull.score);
        assert!((bull.score + bear.score).abs() < 0.01, "正负应对称");
        assert!(bear.label.contains("恐慌") || bear.label.contains("偏空"));
    }

    #[test]
    fn test_sentiment_clamped() {
        // 极端暴涨也不应超过 100
        let s = calc_sentiment(vec![comp(9.9, 1.0)]);
        assert!(s.score <= 100.0 && s.score > 95.0);
    }

    #[test]
    fn test_sentiment_empty() {
        let s = calc_sentiment(vec![]);
        assert_eq!(s.score, 0.0);
    }
}

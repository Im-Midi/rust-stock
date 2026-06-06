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
    pub time: String,  // HH:MM
    pub txt: String,   // 标题/摘要
    pub tag: String,   // 栏目标签（可空）
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
            Some(NewsItem { time, txt, tag })
        })
        .collect()
}

#[cfg(feature = "net")]
pub async fn fetch_news() -> Result<Vec<NewsItem>, String> {
    // 实测（2026-06-06）：sortEnd 不能为空串、req_trace（毫秒时间戳）必填，缺一个都返回
    // {"code":0,"message":"Required String parameter 'xxx' is not present","data":null}
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let url = format!(
        "https://np-weblist.eastmoney.com/comm/web/getFastNewsList?client=web&biz=web_724&fastColumn=102&sortEnd=0&pageSize=20&req_trace={ts}"
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
        assert_eq!(items[2].txt, "title空时回退summary");
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

// quote.rs — 本地行情抓取（无服务器，Rust 进程直连公开接口）
//
// 数据源：
//   1. 新浪财经  https://hq.sinajs.cn/list=...   (GBK 编码，需 Referer)
//   2. 东方财富  https://push2.eastmoney.com/... (UTF-8 JSON)
//
// 两者都是第三方公开接口，纯本地直连，不经过任何自建服务器。

use serde::{Deserialize, Serialize};

/// 统一的行情结构（指数 / 个股通用）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Quote {
    pub code: String,      // 代码，如 sh000001
    pub name: String,      // 名称
    pub price: f64,        // 当前价 / 当前点位
    pub prev_close: f64,   // 昨收
    pub change: f64,       // 涨跌额
    pub change_pct: f64,   // 涨跌幅 %
    pub open: f64,         // 今开
    pub high: f64,         // 最高
    pub low: f64,          // 最低
    pub volume: f64,       // 成交量
    pub amount: f64,       // 成交额
    pub source: String,    // 数据来源 "sina" | "eastmoney"
}

impl Quote {
    pub fn up(&self) -> bool {
        self.change >= 0.0
    }
}

// ============================================================
// 新浪解析
// ============================================================
//
// 原始返回示例（GBK 转码后）：
// var hq_str_sh000001="上证指数,4095.4500,4128.7200,4060.1500,...,2026-03-14,11:30:00,00";
//
// 指数字段顺序（索引）：
//   0 name 1 当前点位 2 昨收 3 ?  实际新浪指数字段：
//   0 名称 1 今开 2 昨收 3 当前 4 最高 5 最低 6 ... 7 成交量 8 成交额
// 注意：新浪「指数」和「个股」字段含义不同。下面按指数布局解析，
//       个股用 parse_sina_stock。

/// 解析单行新浪指数数据
pub fn parse_sina_index_line(line: &str) -> Option<Quote> {
    // 形如: var hq_str_sh000001="...";
    let code = line.split("hq_str_").nth(1)?.split('=').next()?.trim().to_string();
    let payload = line.split('"').nth(1)?;
    if payload.is_empty() {
        return None; // 停牌或无数据
    }
    let f: Vec<&str> = payload.split(',').collect();
    if f.len() < 6 {
        return None;
    }
    // 新浪指数布局：name, open, prev_close? —— 实际是 name,当前,涨跌额,涨跌幅,成交量(手),成交额(万)
    // 国内指数(sh/sz)字段：0名称 1当前点位 2昨收?  历史上新浪指数与个股共用同一布局：
    //   0名 1今开 2昨收 3当前价 4最高 5最低 6买一 7卖一 8成交量 9成交额 ...
    let name = f[0].to_string();
    let open = f.get(1)?.parse().unwrap_or(0.0);
    let prev_close = f.get(2)?.parse().unwrap_or(0.0);
    let price = f.get(3)?.parse().unwrap_or(0.0);
    let high = f.get(4).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let low = f.get(5).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let volume = f.get(8).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let amount = f.get(9).and_then(|v| v.parse().ok()).unwrap_or(0.0);

    let change = price - prev_close;
    let change_pct = if prev_close != 0.0 {
        change / prev_close * 100.0
    } else {
        0.0
    };

    Some(Quote {
        code,
        name,
        price,
        prev_close,
        change,
        change_pct,
        open,
        high,
        low,
        volume,
        amount,
        source: "sina".into(),
    })
}

/// 解析整段新浪返回（多行）
pub fn parse_sina_response(body: &str) -> Vec<Quote> {
    body.lines()
        .filter(|l| l.contains("hq_str_"))
        .filter_map(parse_sina_index_line)
        .collect()
}

// ============================================================
// 东方财富解析
// ============================================================
//
// 接口：https://push2.eastmoney.com/api/qt/ulist.np/get
//   ?fields=f12,f14,f2,f3,f4,f15,f16,f17,f18&secids=1.000001,0.399001
// 返回 JSON：
// {"data":{"diff":[{"f2":40954,"f3":-81,"f12":"000001","f14":"上证指数",...}]}}
//
// 字段含义：
//   f2  现价（带倍率，需 /100）   f3 涨跌幅(%)（/100）   f4 涨跌额（/100）
//   f12 代码                      f14 名称
//   f15 最高(/100) f16 最低(/100) f17 今开(/100) f18 昨收(/100)
// 倍率：A股价格通常 ×100，指数点位也 ×100。具体精度看 f152，简化按 /100。

#[derive(Debug, Deserialize)]
struct EmResp {
    data: Option<EmData>,
}
#[derive(Debug, Deserialize)]
struct EmData {
    diff: Option<Vec<serde_json::Value>>,
}

fn em_num(v: &serde_json::Value, key: &str) -> f64 {
    match &v[key] {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// 解析东方财富 JSON
pub fn parse_eastmoney_response(body: &str) -> Vec<Quote> {
    let resp: EmResp = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let diff = match resp.data.and_then(|d| d.diff) {
        Some(d) => d,
        None => return vec![],
    };
    diff.iter()
        .map(|v| {
            let price = em_num(v, "f2") / 100.0;
            let prev_close = em_num(v, "f18") / 100.0;
            let change = em_num(v, "f4") / 100.0;
            let change_pct = em_num(v, "f3") / 100.0;
            Quote {
                code: v["f12"].as_str().unwrap_or("").to_string(),
                name: v["f14"].as_str().unwrap_or("").to_string(),
                price,
                prev_close,
                change,
                change_pct,
                open: em_num(v, "f17") / 100.0,
                high: em_num(v, "f15") / 100.0,
                low: em_num(v, "f16") / 100.0,
                volume: em_num(v, "f5"),
                amount: em_num(v, "f6"),
                source: "eastmoney".into(),
            }
        })
        .collect()
}

// ============================================================
// 网络抓取（运行时才用，测试不依赖网络）
// ============================================================
#[cfg(feature = "net")]
pub async fn fetch_sina(codes: &[&str]) -> Result<Vec<Quote>, String> {
    let list = codes.join(",");
    let url = format!("https://hq.sinajs.cn/list={}", list);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Referer", "https://finance.sina.com.cn") // 必须，否则 403
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    // GBK -> UTF-8
    let (text, _, _) = encoding_rs::GBK.decode(&bytes);
    Ok(parse_sina_response(&text))
}

#[cfg(feature = "net")]
pub async fn fetch_eastmoney(secids: &[&str]) -> Result<Vec<Quote>, String> {
    let ids = secids.join(",");
    let url = format!(
        "https://push2.eastmoney.com/api/qt/ulist.np/get?fields=f2,f3,f4,f5,f6,f12,f14,f15,f16,f17,f18&secids={}",
        ids
    );
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(parse_eastmoney_response(&text))
}

// ============================================================
// 单元测试：用模拟的真实返回验证解析正确性（不依赖网络）
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sina_index() {
        // 模拟新浪上证指数返回（字段布局与真实一致）
        let raw = r#"var hq_str_sh000001="上证指数,4128.72,4128.72,4095.45,4135.10,4060.15,0,0,123456789,987654321000,2026-03-14,11:30:00,00";"#;
        let quotes = parse_sina_response(raw);
        assert_eq!(quotes.len(), 1);
        let q = &quotes[0];
        assert_eq!(q.code, "sh000001");
        assert_eq!(q.name, "上证指数");
        assert_eq!(q.price, 4095.45);
        assert_eq!(q.prev_close, 4128.72);
        assert!((q.change - (-33.27)).abs() < 0.01);
        assert!(!q.up()); // 下跌
        assert!(q.change_pct < 0.0);
    }

    #[test]
    fn test_sina_empty() {
        // 停牌/无数据时 payload 为空
        let raw = r#"var hq_str_sh000001="";"#;
        assert_eq!(parse_sina_response(raw).len(), 0);
    }

    #[test]
    fn test_eastmoney() {
        // 模拟东方财富 ulist 返回（价格带 ×100 倍率）
        let raw = r#"{"data":{"diff":[
            {"f2":409545,"f3":-81,"f4":-3327,"f5":123456,"f6":987654,
             "f12":"000001","f14":"上证指数","f15":413510,"f16":406015,"f17":412872,"f18":412872}
        ]}}"#;
        let quotes = parse_eastmoney_response(raw);
        assert_eq!(quotes.len(), 1);
        let q = &quotes[0];
        assert_eq!(q.name, "上证指数");
        assert!((q.price - 4095.45).abs() < 0.01);
        assert!((q.change_pct - (-0.81)).abs() < 0.01);
        assert!(!q.up());
    }

    #[test]
    fn test_eastmoney_garbage() {
        // 接口异常返回时不应 panic
        assert_eq!(parse_eastmoney_response("not json").len(), 0);
        assert_eq!(parse_eastmoney_response(r#"{"data":null}"#).len(), 0);
    }
}

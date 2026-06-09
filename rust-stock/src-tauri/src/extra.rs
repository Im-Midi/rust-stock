// extra.rs — 板块热力 + 个股资金流（东方财富 push2，UTF-8 JSON）
// 字段按东财公开约定（fltt=2 时百分比/价格为真实值，不带 ×100）。

use serde::Serialize;

fn num(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

// ============================================================
// 板块热力（行业板块涨跌幅）
// ============================================================
#[derive(Debug, Clone, Serialize)]
pub struct Sector {
    pub name: String,
    pub code: String,
    pub change_pct: f64,
}

/// 解析东财 clist：{"data":{"diff":[{"f3":3.21,"f12":"BK0475","f14":"半导体"}]}}
/// diff 可能是对象数组，也可能是 {"0":{...}} 形式，两种都处理。
pub fn parse_sectors(body: &str) -> Vec<Sector> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let diff = &v["data"]["diff"];
    let items: Vec<&serde_json::Value> = if let Some(a) = diff.as_array() {
        a.iter().collect()
    } else if let Some(o) = diff.as_object() {
        o.values().collect()
    } else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|d| {
            let name = d["f14"].as_str()?.to_string();
            if name.is_empty() {
                return None;
            }
            Some(Sector {
                name,
                code: d["f12"].as_str().unwrap_or("").to_string(),
                change_pct: num(&d["f3"]),
            })
        })
        .collect()
}

// 取错误链最内层（真实根因）。reqwest 外层常是 "error sending request for url"，
// 真正原因（dns/tls/连接重置/超时）在 source 链更深处。
fn root_cause(e: &dyn std::error::Error) -> String {
    let mut msg = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        msg = s.to_string();
        src = s.source();
    }
    msg
}

#[cfg(feature = "net")]
pub async fn fetch_sectors() -> Result<Vec<Sector>, String> {
    // m:90 t:2 = 行业板块；按涨跌幅 f3 降序
    let url = "https://push2.eastmoney.com/api/qt/clist/get?pn=1&pz=60&po=1&np=1&fltt=2&invt=2&fid=f3&fs=m:90+t:2&fields=f3,f12,f14&ut=bd1d9ddb04089700cf9c27f6f7426281";
    // 配 UA + 超时；启动时多请求并发，连接易抖动 → 重试 3 次
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/120 Mobile Safari/537.36")
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| format!("板块客户端构建失败: {e}"))?;
    let mut last = String::from("未知错误");
    for attempt in 0..3 {
        match client
            .get(url)
            .header("Referer", "https://quote.eastmoney.com/")
            .send()
            .await
        {
            Ok(resp) => match resp.text().await {
                Ok(text) => {
                    let list = parse_sectors(&text);
                    if !list.is_empty() {
                        return Ok(list);
                    }
                    last = "板块解析为空（接口字段可能变了）".into();
                }
                Err(e) => last = format!("读取响应失败: {}", root_cause(&e)),
            },
            Err(e) => last = format!("板块请求失败: {}", root_cause(&e)),
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
    Err(last)
}

// ============================================================
// 个股资金流 + 扩展行情快照
// ============================================================
#[derive(Debug, Clone, Serialize)]
pub struct FundFlow {
    pub name: String,
    pub code: String,
    pub main: f64,           // 主力净流入（元）
    pub main_pct: f64,       // 主力净占比 %
    pub super_big: f64,      // 超大单净额
    pub super_big_pct: f64,  // 超大单净占比 %
    pub big: f64,            // 大单
    pub big_pct: f64,        // 大单净占比 %
    pub mid: f64,            // 中单
    pub mid_pct: f64,        // 中单净占比 %
    pub small: f64,          // 小单
    pub small_pct: f64,      // 小单净占比 %
    pub turnover: f64,       // 换手率 %（f8）
}

/// 解析东财资金流 ulist.np/get：{"data":{"diff":[{...}]}}（与行情同结构）
/// f12 代码, f14 名称, f62 主力净额(元), f184 主力净占比%,
/// f66 超大单净额, f72 大单, f78 中单, f84 小单
pub fn parse_fund_flow(body: &str) -> Option<FundFlow> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let diff = &v["data"]["diff"];
    let d = if let Some(a) = diff.as_array() {
        a.first()?.clone()
    } else if let Some(o) = diff.as_object() {
        o.values().next()?.clone()
    } else {
        return None;
    };
    Some(FundFlow {
        name: d["f14"].as_str().unwrap_or("").to_string(),
        code: d["f12"].as_str().unwrap_or("").to_string(),
        main: num(&d["f62"]),
        main_pct: num(&d["f184"]),
        super_big: num(&d["f66"]),
        super_big_pct: num(&d["f69"]),
        big: num(&d["f72"]),
        big_pct: num(&d["f75"]),
        mid: num(&d["f78"]),
        mid_pct: num(&d["f81"]),
        small: num(&d["f84"]),
        small_pct: num(&d["f87"]),
        turnover: num(&d["f8"]),
    })
}

#[cfg(feature = "net")]
pub async fn fetch_fund_flow(code: &str) -> Result<FundFlow, String> {
    let secid = crate::sources::to_secid(code).ok_or_else(|| format!("无法识别代码: {code}"))?;
    // 用资金流专用 ulist.np/get（与行情同接口，换资金流字段），stock/get 不返回资金流
    let url = format!(
        "https://push2.eastmoney.com/api/qt/ulist.np/get?fltt=2&invt=2&secids={secid}\
         &fields=f8,f12,f14,f62,f184,f66,f69,f72,f75,f78,f81,f84,f87"
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Referer", "https://quote.eastmoney.com/")
        .send()
        .await
        .map_err(|e| format!("资金流请求失败: {e}"))?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    parse_fund_flow(&text).ok_or_else(|| "资金流解析为空".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sectors() {
        let raw = r#"{"data":{"diff":[
            {"f3":3.21,"f12":"BK0475","f14":"半导体"},
            {"f3":-1.50,"f12":"BK0438","f14":"煤炭"},
            {"f3":0.0,"f12":"BK0001","f14":""}
        ]}}"#;
        let s = parse_sectors(raw);
        assert_eq!(s.len(), 2); // 空名过滤
        assert_eq!(s[0].name, "半导体");
        assert!((s[0].change_pct - 3.21).abs() < 0.001);
        assert!((s[1].change_pct + 1.50).abs() < 0.001);
        assert_eq!(parse_sectors("x").len(), 0);
    }

    #[test]
    fn test_parse_sectors_object_diff() {
        // diff 为对象形式
        let raw = r#"{"data":{"diff":{"0":{"f3":2.0,"f12":"BK1","f14":"AI"}}}}"#;
        assert_eq!(parse_sectors(raw).len(), 1);
    }

    #[test]
    fn test_parse_fund_flow() {
        // 东财资金流 ulist 真实形状（元为单位）：主力 -2.9145亿
        let raw = r#"{"data":{"diff":[{"f8":1.85,"f12":"600519","f14":"贵州茅台","f62":-291450000.0,"f184":-2.44,"f66":-742950000.0,"f69":-6.22,"f72":451500000.0,"f75":3.78,"f78":309150000.0,"f81":2.59,"f84":-17702336.0,"f87":-0.15}]}}"#;
        let f = parse_fund_flow(raw).unwrap();
        assert_eq!(f.name, "贵州茅台");
        assert!((f.main - (-291450000.0)).abs() < 1.0);
        assert!((f.main_pct - (-2.44)).abs() < 0.01);
        assert!((f.big - 451500000.0).abs() < 1.0);
        assert!((f.big_pct - 3.78).abs() < 0.01);
        assert!((f.turnover - 1.85).abs() < 0.01);
        assert!(parse_fund_flow(r#"{"data":{"diff":[]}}"#).is_none());
        assert!(parse_fund_flow("x").is_none());
    }
}

// ============================================================
// 今日推荐候选池（涨幅榜 + 主力净流入榜 + 龙虎榜合并）
// ============================================================
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub code: String,        // 统一格式 sh600519
    pub name: String,
    pub price: f64,
    pub change_pct: f64,
    pub turnover: f64,       // 换手率 %
    pub main_flow: f64,      // 主力净流入（元）
    pub on_lhb: bool,        // 是否当日上龙虎榜
}

/// 解析东财 clist 个股榜：data.diff[]，字段 f12代码 f13市场 f14名 f2现价 f3涨跌 f8换手 f62主力净额
fn parse_clist_stocks(body: &str) -> Vec<Candidate> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let diff = &v["data"]["diff"];
    let items: Vec<&serde_json::Value> = if let Some(a) = diff.as_array() {
        a.iter().collect()
    } else if let Some(o) = diff.as_object() {
        o.values().collect()
    } else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|d| {
            let code6 = d["f12"].as_str()?;
            let mkt = d["f13"].as_i64().unwrap_or(-1);
            let prefix = match mkt {
                1 => "sh",
                0 => "sz",
                _ => return None,
            };
            let name = d["f14"].as_str().unwrap_or("").to_string();
            if name.is_empty() || name.contains("ST") {
                return None; // 过滤 ST
            }
            Some(Candidate {
                code: format!("{prefix}{code6}"),
                name,
                price: num(&d["f2"]),
                change_pct: num(&d["f3"]),
                turnover: num(&d["f8"]),
                main_flow: num(&d["f62"]),
                on_lhb: false,
            })
        })
        .collect()
}

#[cfg(feature = "net")]
async fn clist_rank(fid: &str, pz: u32) -> Vec<Candidate> {
    // 沪深主板+创业板+科创板 A 股
    let url = format!(
        "https://push2.eastmoney.com/api/qt/clist/get?pn=1&pz={pz}&po=1&np=1&fltt=2&invt=2\
         &fid={fid}&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23,m:0+t:7,m:1+t:3\
         &fields=f2,f3,f8,f12,f13,f14,f62&ut=bd1d9ddb04089700cf9c27f6f7426281"
    );
    match reqwest::Client::new()
        .get(&url)
        .header("Referer", "https://quote.eastmoney.com/")
        .send()
        .await
    {
        Ok(resp) => match resp.text().await {
            Ok(t) => parse_clist_stocks(&t),
            Err(_) => vec![],
        },
        Err(_) => vec![],
    }
}

/// 解析龙虎榜返回，取当日上榜的 6 位代码集合
fn parse_lhb_codes(body: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(arr) = v["result"]["data"].as_array() {
            for item in arr {
                if let Some(c) = item["SECURITY_CODE"].as_str() {
                    set.insert(c.to_string());
                }
            }
        }
    }
    set
}

#[cfg(feature = "net")]
async fn fetch_lhb_codes() -> std::collections::HashSet<String> {
    // 东财龙虎榜当日列表（取最近交易日，按上榜净额）
    let url = "https://datacenter-web.eastmoney.com/api/data/v1/get?sortColumns=TRADE_DATE&sortTypes=-1&pageSize=200&pageNumber=1&reportName=RPT_DAILYBILLBOARD_DETAILSNEW&columns=SECURITY_CODE&source=WEB&client=WEB";
    match reqwest::Client::new().get(url).header("Referer", "https://data.eastmoney.com/").send().await {
        Ok(resp) => match resp.text().await {
            Ok(t) => parse_lhb_codes(&t),
            Err(_) => Default::default(),
        },
        Err(_) => Default::default(),
    }
}

/// 候选池：涨幅榜 top + 主力净流入榜 top 合并去重，标记龙虎榜
#[cfg(feature = "net")]
pub async fn fetch_candidates() -> Result<Vec<Candidate>, String> {
    let (gainers, inflow, lhb) = tokio::join!(
        clist_rank("f3", 35),   // 涨幅榜
        clist_rank("f62", 35),  // 主力净流入榜
        fetch_lhb_codes(),
    );
    let mut map: std::collections::BTreeMap<String, Candidate> = std::collections::BTreeMap::new();
    for mut c in gainers.into_iter().chain(inflow.into_iter()) {
        // 龙虎榜标记（code 去前缀比对 6 位）
        let code6 = &c.code[2..];
        c.on_lhb = lhb.contains(code6);
        map.entry(c.code.clone()).or_insert(c);
    }
    let out: Vec<Candidate> = map.into_v
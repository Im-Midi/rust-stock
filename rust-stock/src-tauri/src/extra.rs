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

#[cfg(feature = "net")]
pub async fn fetch_sectors() -> Result<Vec<Sector>, String> {
    // m:90 t:2 = 行业板块；按涨跌幅 f3 降序
    let url = "https://push2.eastmoney.com/api/qt/clist/get?pn=1&pz=60&po=1&np=1&fltt=2&invt=2&fid=f3&fs=m:90+t:2&fields=f3,f12,f14&ut=bd1d9ddb04089700cf9c27f6f7426281";
    let resp = reqwest::Client::new()
        .get(url)
        .header("Referer", "https://quote.eastmoney.com/")
        .send()
        .await
        .map_err(|e| format!("板块请求失败: {e}"))?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let list = parse_sectors(&text);
    if list.is_empty() {
        return Err("板块解析为空（接口字段可能变了）".into());
    }
    Ok(list)
}

// ============================================================
// 个股资金流 + 扩展行情快照
// ============================================================
#[derive(Debug, Clone, Serialize)]
pub struct FundFlow {
    pub name: String,
    pub code: String,
    pub main: f64,       // 主力净流入（元）
    pub main_pct: f64,   // 主力净占比 %
    pub super_big: f64,  // 超大单净额
    pub big: f64,        // 大单
    pub mid: f64,        // 中单
    pub small: f64,      // 小单
}

/// 解析东财 stock/get 资金流：data 对象
/// f62 主力净额, f184 主力净占比, f66 超大单, f72 大单, f78 中单, f84 小单, f57 代码, f58 名称
pub fn parse_fund_flow(body: &str) -> Option<FundFlow> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let d = &v["data"];
    if d.is_null() {
        return None;
    }
    let name = d["f58"].as_str().unwrap_or("").to_string();
    Some(FundFlow {
        name,
        code: d["f57"].as_str().unwrap_or("").to_string(),
        main: num(&d["f62"]),
        main_pct: num(&d["f184"]),
        super_big: num(&d["f66"]),
        big: num(&d["f72"]),
        mid: num(&d["f78"]),
        small: num(&d["f84"]),
    })
}

#[cfg(feature = "net")]
pub async fn fetch_fund_flow(code: &str) -> Result<FundFlow, String> {
    let secid = crate::sources::to_secid(code).ok_or_else(|| format!("无法识别代码: {code}"))?;
    let url = format!(
        "https://push2.eastmoney.com/api/qt/stock/get?secid={secid}&fltt=2&invt=2\
         &fields=f57,f58,f62,f184,f66,f72,f78,f84&ut=fa5fd1943c7b386f172d6893dbfba10b"
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
        let raw = r#"{"data":{"f57":"600519","f58":"贵州茅台","f62":12345678.0,"f184":5.2,"f66":8000000.0,"f72":4345678.0,"f78":-2000000.0,"f84":-1000000.0}}"#;
        let f = parse_fund_flow(raw).unwrap();
        assert_eq!(f.name, "贵州茅台");
        assert!((f.main - 12345678.0).abs() < 1.0);
        assert!((f.main_pct - 5.2).abs() < 0.01);
        assert!(parse_fund_flow(r#"{"data":null}"#).is_none());
    }
}

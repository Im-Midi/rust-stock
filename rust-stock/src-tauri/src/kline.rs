// kline.rs — 历史K线（东方财富公开接口）
//
// 接口：https://push2his.eastmoney.com/api/qt/stock/kline/get
//   ?secid=1.600519&fields1=f1,f2,f3,f4,f5,f6&fields2=f51,f52,f53,f54,f55,f56
//   &klt=101&fqt=1&end=20500101&lmt=90
// klt: 101=日K 102=周K 103=月K；fqt=1 前复权
// 返回 data.klines: ["2026-06-06,1685.00,1690.00,1700.00,1680.00,12345", ...]
//   字段顺序（fields2 对应）：f51日期 f52开 f53收 f54高 f55低 f56量

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Candle {
    pub date: String,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    pub amount: f64,   // 成交额（元）f57
    pub turnover: f64, // 换手率（%）f61
}

pub fn parse_kline(body: &str) -> Vec<Candle> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let klines = match v["data"]["klines"].as_array() {
        Some(k) => k,
        None => return vec![],
    };
    klines
        .iter()
        .filter_map(|line| {
            let s = line.as_str()?;
            let f: Vec<&str> = s.split(',').collect();
            if f.len() < 6 {
                return None;
            }
            Some(Candle {
                date: f[0].to_string(),
                open: f[1].parse().ok()?,
                close: f[2].parse().ok()?,
                high: f[3].parse().ok()?,
                low: f[4].parse().ok()?,
                volume: f[5].parse().unwrap_or(0.0),
                amount: f.get(6).and_then(|x| x.parse().ok()).unwrap_or(0.0),
                turnover: f.get(7).and_then(|x| x.parse().ok()).unwrap_or(0.0),
            })
        })
        .collect()
}

#[cfg(feature = "net")]
pub async fn fetch_kline(code: &str, klt: u32, lmt: u32) -> Result<Vec<Candle>, String> {
    let secid = crate::sources::to_secid(code).ok_or_else(|| format!("无法识别的代码: {code}"))?;
    let url = format!(
        "https://push2his.eastmoney.com/api/qt/stock/kline/get?secid={secid}\
         &fields1=f1,f2,f3,f4,f5,f6&fields2=f51,f52,f53,f54,f55,f56,f57,f61\
         &klt={klt}&fqt=1&end=20500101&lmt={lmt}"
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("K线请求失败: {e}"))?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let candles = parse_kline(&text);
    if candles.is_empty() {
        return Err("K线解析为空（代码不存在或接口字段变了）".into());
    }
    Ok(candles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_kline() {
        let raw = r#"{"data":{"code":"600519","name":"贵州茅台","klines":[
            "2026-06-04,1680.00,1690.50,1702.00,1675.30,32100",
            "2026-06-05,1691.00,1685.20,1695.00,1678.00,28000",
            "bad,line",
            "2026-06-06,1686.00,1701.00,1705.50,1684.00,35500"
        ]}}"#;
        let c = parse_kline(raw);
        assert_eq!(c.len(), 3); // 坏行被过滤
        assert_eq!(c[0].date, "2026-06-04");
        assert!((c[0].open - 1680.0).abs() < 0.01);
        assert!((c[2].close - 1701.0).abs() < 0.01);
        assert!(c[2].close > c[2].open); // 阳线
        assert_eq!(c[1].volume, 28000.0);
    }

    #[test]
    fn test_parse_kline_garbage() {
        assert_eq!(parse_kline("not json").len(), 0);
        assert_eq!(parse_kline(r#"{"data":null}"#).len(), 0);
    }
}

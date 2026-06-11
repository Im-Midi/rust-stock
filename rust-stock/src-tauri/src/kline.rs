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

/// 解析腾讯前复权日K：data.{code}.qfqday = [[日期,开,收,高,低,量,(可选分红)],...]
pub fn parse_tencent_kline(body: &str, code: &str) -> Vec<Candle> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let node = &v["data"][code];
    let arr = match ["qfqday", "day", "qfqweek", "week", "qfqmonth", "month"]
        .iter()
        .find_map(|k| node[*k].as_array())
    {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter()
        .filter_map(|row| {
            let r = row.as_array()?;
            if r.len() < 6 {
                return None;
            }
            let num = |i: usize| r.get(i).and_then(|x| x.as_str()).and_then(|s| s.parse::<f64>().ok());
            Some(Candle {
                date: r.get(0)?.as_str()?.to_string(),
                open: num(1)?,
                close: num(2)?,
                high: num(3)?,
                low: num(4)?,
                volume: num(5).unwrap_or(0.0),
                amount: 0.0,   // 腾讯不带成交额
                turnover: 0.0, // 腾讯日K不带换手率（筹码退化为成交量分布，价格仍真实）
            })
        })
        .collect()
}

/// 解析新浪日K：[{"day":"... HH:MM:SS","open":"..","high":"..","low":"..","close":"..","volume":".."}]
pub fn parse_sina_kline(body: &str) -> Vec<Candle> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match v.as_array() {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter()
        .filter_map(|it| {
            let num = |k: &str| it[k].as_str().and_then(|s| s.parse::<f64>().ok());
            let day = it["day"].as_str()?;
            Some(Candle {
                date: day.split(' ').next().unwrap_or(day).to_string(),
                open: num("open")?,
                close: num("close")?,
                high: num("high")?,
                low: num("low")?,
                volume: num("volume").unwrap_or(0.0),
                amount: 0.0,
                turnover: 0.0,
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
    // 1) 东方财富（最全：带成交额/换手率，筹码精度最高）。免 gzip + 3 次重试化解安卓 rustls close_notify
    if let Ok(text) = crate::extra::em_get_text(&url, "https://quote.eastmoney.com/").await {
        let c = parse_kline(&text);
        if !c.is_empty() {
            return Ok(c);
        }
    }
    // 2) 腾讯（前复权日/周/月，价格真实；无换手率/成交额）
    let t = fetch_kline_tencent(code, klt, lmt).await;
    if !t.is_empty() {
        return Ok(t);
    }
    // 3) 新浪（日K兜底）
    let s = fetch_kline_sina(code, klt, lmt).await;
    if !s.is_empty() {
        return Ok(s);
    }
    Err("K线获取失败（东财/腾讯/新浪三源均未取到，请稍后重试）".into())
}

#[cfg(feature = "net")]
async fn fetch_kline_tencent(code: &str, klt: u32, lmt: u32) -> Vec<Candle> {
    let period = match klt {
        102 => "week",
        103 => "month",
        _ => "day",
    };
    let lc = code.to_lowercase();
    let url = format!("https://web.ifzq.gtimg.cn/appstock/app/fqkline/get?param={lc},{period},,,{lmt},qfq");
    match crate::extra::em_get_text(&url, "https://gu.qq.com/").await {
        Ok(text) => parse_tencent_kline(&text, &lc),
        Err(_) => vec![],
    }
}

#[cfg(feature = "net")]
async fn fetch_kline_sina(code: &str, klt: u32, lmt: u32) -> Vec<Candle> {
    let scale = match klt {
        102 => 1680,
        103 => 7200,
        _ => 240,
    };
    let lc = code.to_lowercase();
    let url = format!(
        "https://money.finance.sina.com.cn/quotes_service/api/json_v2.php/CN_MarketData.getKLineData?symbol={lc}&scale={scale}&ma=no&datalen={lmt}"
    );
    match crate::extra::em_get_text(&url, "https://finance.sina.com.cn/").await {
        Ok(text) => parse_sina_kline(&text),
        Err(_) => vec![],
    }
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

    #[test]
    fn test_parse_tencent_kline() {
        let raw = r#"{"code":0,"data":{"sh600782":{"qfqday":[
            ["2026-06-10","2.610","2.590","2.620","2.560","395433.000"],
            ["2026-06-11","2.570","2.590","2.610","2.550","266333.000",{"nd":"x"}]
        ]}}}"#;
        let c = parse_tencent_kline(raw, "sh600782");
        assert_eq!(c.len(), 2);
        assert!((c[1].close - 2.59).abs() < 1e-6);
        assert!((c[1].open - 2.57).abs() < 1e-6);
        assert!((c[1].high - 2.61).abs() < 1e-6);
        assert!((c[1].low - 2.55).abs() < 1e-6);
        assert!(parse_tencent_kline("x", "sh1").is_empty());
    }

    #[test]
    fn test_parse_sina_kline() {
        let raw = r#"[{"day":"2026-06-11 15:00:00","open":"2.570","high":"2.610","low":"2.550","close":"2.590","volume":"26633300"}]"#;
        let c = parse_sina_kline(raw);
        assert_eq!(c.len(), 1);
        assert!((c[0].close - 2.59).abs() < 1e-6);
        assert_eq!(c[0].date, "2026-06-11");
        assert!(parse_sina_kline("{}").is_empty());
    }
}

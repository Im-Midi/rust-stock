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

// 东财专用客户端：关 gzip → 响应带 Content-Length、明文，可干净读完；
// （gzip 流式响应在 rustls 下，服务器不发 close_notify 收尾会被误判为错误）。
#[cfg(feature = "net")]
fn em_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/120 Mobile Safari/537.36")
        .timeout(std::time::Duration::from_secs(12))
        .no_gzip()
        .pool_max_idle_per_host(0)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

// 东财 GET（带重试）：rustls 下东财偶发 "peer closed connection without
// sending tls close_notify" / 连接重置，多为瞬时，重试 3 次基本可恢复。
#[cfg(feature = "net")]
pub(crate) async fn em_get_text(url: &str, referer: &str) -> Result<String, String> {
    let mut last = "未知错误".to_string();
    for attempt in 0u32..3 {
        match em_client().get(url).header("Referer", referer).send().await {
            Ok(resp) => match resp.text().await {
                Ok(t) if !t.trim().is_empty() => return Ok(t),
                Ok(_) => last = "响应为空".into(),
                Err(e) => last = root_cause(&e),
            },
            Err(e) => last = root_cause(&e),
        }
        // 退避 150ms / 350ms（最后一次失败不再 sleep）
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(150 + attempt as u64 * 200)).await;
        }
    }
    Err(last)
}

#[cfg(feature = "net")]
pub async fn fetch_sectors() -> Result<Vec<Sector>, String> {
    // clist 板块列表接口在 rustls 下报 close_notify；改用 ulist.np（与资金流同接口，正常）
    // + 固定主要行业板块 secid 列表拉实时涨跌，板块名称由接口返回。
    let secids = "90.BK0475,90.BK0464,90.BK0727,90.BK0473,90.BK0438,90.BK0447,90.BK0448,90.BK0428,90.BK0737,90.BK0421,90.BK0477,90.BK0459,90.BK0729,90.BK0910";
    let url = format!(
        "https://push2.eastmoney.com/api/qt/ulist.np/get?fltt=2&invt=2&secids={secids}&fields=f2,f3,f12,f14"
    );
    let text = em_get_text(&url, "https://quote.eastmoney.com/")
        .await
        .map_err(|e| format!("板块请求失败: {e}"))?;
    let mut list = parse_sectors(&text);
    if list.is_empty() {
        return Err("板块解析为空".into());
    }
    list.sort_by(|a, b| b.change_pct.partial_cmp(&a.change_pct).unwrap_or(std::cmp::Ordering::Equal));
    Ok(list)
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
    // 竞速取数：东财（5 档资金流 + 换手率，最全）对阵延迟 2 秒起跑的腾讯兜底
    //（只有换手率，5 档置 0）。东财健康时 2 秒内回包赢得竞速；东财被掐时
    // 腾讯很快顶上，K线详情页不再等满东财整条重试链（最坏 30 秒+）。
    let em = async {
        match em_get_text(&url, "https://quote.eastmoney.com/").await {
            Ok(text) => parse_fund_flow(&text),
            Err(_) => None,
        }
    };
    let backup = async {
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
        fetch_turnover_tencent(code).await
    };
    tokio::pin!(em);
    tokio::pin!(backup);
    let (mut em_done, mut backup_done) = (false, false);
    while !(em_done && backup_done) {
        tokio::select! {
            r = &mut em, if !em_done => {
                em_done = true;
                if let Some(ff) = r {
                    return Ok(ff);
                }
            }
            r = &mut backup, if !backup_done => {
                backup_done = true;
                if let Some(ff) = r {
                    return Ok(ff);
                }
            }
        }
    }
    Err("资金流/换手率获取失败（东财与腾讯均未取到）".into())
}

/// 解析腾讯轻量报价 v_xxx="a~b~..."；索引 38=换手率%（5档资金流腾讯无，置 0）
fn parse_gtimg_turnover(text: &str, code: &str) -> Option<FundFlow> {
    let start = text.find('"')? + 1;
    let end = text.rfind('"')?;
    if end <= start {
        return None;
    }
    let fields: Vec<&str> = text[start..end].split('~').collect();
    if fields.len() < 39 {
        return None;
    }
    let turnover = fields.get(38).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let name = fields.get(1).map(|s| s.to_string()).unwrap_or_default();
    Some(FundFlow {
        name,
        code: code.to_string(),
        main: 0.0, main_pct: 0.0, super_big: 0.0, super_big_pct: 0.0,
        big: 0.0, big_pct: 0.0, mid: 0.0, mid_pct: 0.0, small: 0.0, small_pct: 0.0,
        turnover,
    })
}

#[cfg(feature = "net")]
async fn fetch_turnover_tencent(code: &str) -> Option<FundFlow> {
    let lc = code.to_lowercase();
    let url = format!("https://qt.gtimg.cn/q={lc}");
    let text = em_get_text(&url, "https://gu.qq.com/").await.ok()?;
    parse_gtimg_turnover(&text, code)
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

    #[test]
    fn test_parse_gtimg_turnover() {
        let mut f: Vec<String> = (0..40).map(|i| i.to_string()).collect();
        f[1] = "新钢股份".into();
        f[38] = "0.85".into();
        let s = format!("v_sh600782=\"{}\";", f.join("~"));
        let ff = parse_gtimg_turnover(&s, "sh600782").unwrap();
        assert!((ff.turnover - 0.85).abs() < 1e-9);
        assert_eq!(ff.code, "sh600782");
        assert!(ff.main.abs() < 1e-9); // 5档资金流置 0
        assert!(parse_gtimg_turnover("bad", "x").is_none());
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
    // 沪深主板+创业板+科创板 A 股（em_get_text：免 gzip + 3 次退避重试，
    // 化解 rustls 偶发 close_notify 把候选池打薄的问题）
    let url = format!(
        "https://push2.eastmoney.com/api/qt/clist/get?pn=1&pz={pz}&po=1&np=1&fltt=2&invt=2\
         &fid={fid}&fs=m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23,m:0+t:7,m:1+t:3\
         &fields=f2,f3,f8,f12,f13,f14,f62&ut=bd1d9ddb04089700cf9c27f6f7426281"
    );
    match em_get_text(&url, "https://quote.eastmoney.com/").await {
        Ok(t) => parse_clist_stocks(&t),
        Err(_) => vec![],
    }
}

/// 解析腾讯全市场榜 getBoardRankList：data.rank_list[]（2026-06-12 实测）
/// 字段：code("sh600519") name zxj(现价) zdf(涨跌幅%) hsl(换手率%) zljlr(主力净流入,万元)
/// 数值全是字符串，num() 已兼容。
fn parse_qq_rank(body: &str) -> Vec<Candidate> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let list = match v["data"]["rank_list"].as_array() {
        Some(l) => l,
        None => return vec![],
    };
    list.iter()
        .filter_map(|d| {
            let code = d["code"].as_str()?.to_lowercase();
            if !(code.starts_with("sh") || code.starts_with("sz")) || code.len() != 8 {
                return None;
            }
            let name = d["name"].as_str().unwrap_or("").to_string();
            if name.is_empty() || name.contains("ST") {
                return None; // 过滤 ST，与东财榜口径一致
            }
            Some(Candidate {
                code,
                name,
                price: num(&d["zxj"]),
                change_pct: num(&d["zdf"]),
                turnover: num(&d["hsl"]),
                main_flow: num(&d["zljlr"]) * 1e4, // 万元 → 元，与东财 f62 口径一致
                on_lhb: false,
            })
        })
        .collect()
}

// 腾讯全市场榜兜底池：该接口 sort_type 实测只认 price / turnover 等少数值
//（zdf / zljlr 等都报"参数错误"），拿不到现成的涨幅榜/主力榜——所以取
// 「成交额榜」前 N 支作活跃股池，回到 fetch_candidates 里本地再按
// 涨幅 / 主力净流入各排一份，候选池口径与东财版保持一致。
#[cfg(feature = "net")]
async fn qq_rank_pool(count: u32) -> Vec<Candidate> {
    let url = format!(
        "https://proxy.finance.qq.com/cgi/cgi-bin/rank/hs/getBoardRankList?board_code=aStock&sort_type=turnover&direct=down&offset=0&count={count}"
    );
    match em_get_text(&url, "https://gu.qq.com/").await {
        Ok(t) => parse_qq_rank(&t),
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
    match em_get_text(url, "https://data.eastmoney.com/").await {
        Ok(t) => parse_lhb_codes(&t),
        Err(_) => Default::default(),
    }
}

/// 候选池：涨幅榜 top + 主力净流入榜 top 合并去重，标记龙虎榜
#[cfg(feature = "net")]
pub async fn fetch_candidates() -> Result<Vec<Candidate>, String> {
    let (mut gainers, mut inflow, lhb) = tokio::join!(
        clist_rank("f3", 35),   // 涨幅榜
        clist_rank("f62", 35),  // 主力净流入榜
        fetch_lhb_codes(),
    );
    // 东财 clist 两榜全空（重试后仍被掐/接口变动）→ 腾讯全市场成交额榜兜底：
    // 取最活跃 80 支，本地按涨幅 / 主力净流入各排 35 支，候选池不再因东财单点故障变空
    if gainers.is_empty() && inflow.is_empty() {
        let pool = qq_rank_pool(80).await;
        if !pool.is_empty() {
            let mut by_chg = pool.clone();
            by_chg.sort_by(|a, b| b.change_pct.partial_cmp(&a.change_pct).unwrap_or(std::cmp::Ordering::Equal));
            by_chg.truncate(35);
            let mut by_flow = pool;
            by_flow.sort_by(|a, b| b.main_flow.partial_cmp(&a.main_flow).unwrap_or(std::cmp::Ordering::Equal));
            by_flow.truncate(35);
            gainers = by_chg;
            inflow = by_flow;
        }
    }
    let mut map: std::collections::BTreeMap<String, Candidate> = std::collections::BTreeMap::new();
    for mut c in gainers.into_iter().chain(inflow.into_iter()) {
        // 龙虎榜标记（code 去前缀比对 6 位）
        let code6 = &c.code[2..];
        c.on_lhb = lhb.contains(code6);
        map.entry(c.code.clone()).or_insert(c);
    }
    let out: Vec<Candidate> = map.into_values().collect();
    if out.is_empty() {
        return Err("候选池为空（榜单接口可能变了或非交易时段）".into());
    }
    Ok(out)
}

#[cfg(test)]
mod cand_tests {
    use super::*;

    #[test]
    fn test_parse_clist_stocks() {
        let raw = r#"{"data":{"diff":[
            {"f2":16.85,"f3":5.2,"f8":3.1,"f12":"600519","f13":1,"f14":"贵州茅台","f62":12345678.0},
            {"f2":9.4,"f3":-1.2,"f8":8.0,"f12":"000001","f13":0,"f14":"平安银行","f62":-5000000.0},
            {"f2":3.2,"f3":1.0,"f8":2.0,"f12":"000007","f13":0,"f14":"ST全新","f62":100.0}
        ]}}"#;
        let c = parse_clist_stocks(raw);
        assert_eq!(c.len(), 2); // ST 过滤
        assert_eq!(c[0].code, "sh600519");
        assert!((c[0].change_pct - 5.2).abs() < 0.01);
        assert_eq!(c[1].code, "sz000001");
        assert!(c[1].change_pct < 0.0); // 下跌股保留（不淘汰）
    }

    #[test]
    fn test_parse_qq_rank() {
        let raw = r#"{"code":0,"msg":"ok","data":{"rank_list":[
            {"code":"sz300308","name":"中际旭创","zxj":"1129.00","zdf":"0.44","hsl":"1.78","zljlr":"-24995.37"},
            {"code":"sh600519","name":"贵州茅台","zxj":"1281.55","zdf":"0.20","hsl":"0.16","zljlr":"-10481.01"},
            {"code":"sz000007","name":"ST全新","zxj":"3.2","zdf":"1.0","hsl":"2.0","zljlr":"100.0"},
            {"code":"bj430047","name":"诺思兰德","zxj":"10","zdf":"1","hsl":"1","zljlr":"1"}
        ]}}"#;
        let c = parse_qq_rank(raw);
        assert_eq!(c.len(), 2); // ST 与北交所过滤
        assert_eq!(c[0].code, "sz300308");
        assert!((c[0].price - 1129.0).abs() < 0.01);
        assert!((c[0].change_pct - 0.44).abs() < 0.01);
        assert!((c[0].turnover - 1.78).abs() < 0.01);
        assert!((c[0].main_flow - (-24995.37 * 1e4)).abs() < 1.0); // 万元 → 元
        assert!(parse_qq_rank("x").is_empty());
    }

    #[test]
    fn test_parse_lhb_codes() {
        let raw = r#"{"result":{"data":[{"SECURITY_CODE":"600519"},{"SECURITY_CODE":"300750"}]}}"#;
        let s = parse_lhb_codes(raw);
        assert!(s.contains("600519"));
        assert_eq!(s.len(), 2);
        assert!(parse_lhb_codes("x").is_empty());
    }
}

// ============================================================
// 三个 A 股数据维度（接口设计参考 1nchaos/adata 的接口字典，Apache-2.0，
// 未复制其代码；全部接口 2026-06-12 经真实请求复核形状）：
//   ① 历史主力资金流（日）  ② 个股所属板块/概念  ③ 北向（沪深港通）成交
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct FlowDay {
    pub date: String,       // YYYY-MM-DD
    pub main: f64,          // 主力净流入（元，= 大单+超大单净额）
    pub main_pct: f64,      // 主力净占比 %
    pub close: f64,         // 收盘价
    pub change_pct: f64,    // 涨跌幅 %
}

/// 解析东财历史资金流 fflow/daykline/get（2026-06-12 实测）：
/// data.klines = ["2026-06-11,-95013600.0,-81509.0,95095120.0,-80124064.0,
///                 -14889536.0,-2.94,-0.00,2.94,-2.48,-0.46,1279.00,0.24,0.00,0.00",…]
/// 列序（fields2=f51..f65）：0日期 1主力净额 2小单 3中单 4大单 5超大单
///   6主力净占比% 7小单% 8中单% 9大单% 10超大单% 11收盘价 12涨跌幅% 13/14恒0
/// （列序经实测校验：主力净额 == 大单+超大单 净额之和）
pub fn parse_flow_days(body: &str) -> Vec<FlowDay> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match v["data"]["klines"].as_array() {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter()
        .filter_map(|row| {
            let s = row.as_str()?;
            let f: Vec<&str> = s.split(',').collect();
            if f.len() < 13 || f[0].len() != 10 {
                return None;
            }
            let p = |i: usize| f[i].parse::<f64>().unwrap_or(0.0);
            Some(FlowDay {
                date: f[0].to_string(),
                main: p(1),
                main_pct: p(6),
                close: p(11),
                change_pct: p(12),
            })
        })
        .collect()
}

/// 近 N 个交易日主力资金流（升序：旧→新，与接口返回一致）
#[cfg(feature = "net")]
pub async fn fetch_flow_history(code: &str, days: u32) -> Result<Vec<FlowDay>, String> {
    let secid = crate::sources::to_secid(code).ok_or_else(|| format!("无法识别代码: {code}"))?;
    let lmt = days.clamp(1, 60);
    let url = format!(
        "https://push2his.eastmoney.com/api/qt/stock/fflow/daykline/get?lmt={lmt}&klt=101\
         &fields1=f1,f2,f3,f7&fields2=f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61,f62,f63,f64,f65\
         &secid={secid}"
    );
    let text = em_get_text(&url, "https://quote.eastmoney.com/")
        .await
        .map_err(|e| format!("历史资金流请求失败: {e}"))?;
    let list = parse_flow_days(&text);
    if list.is_empty() {
        return Err("历史资金流解析为空".into());
    }
    Ok(list)
}

// ------------------------------------------------------------
// 个股所属板块 / 概念（东财 F10 RPT_F10_CORETHEME_BOARDTYPE）
// ------------------------------------------------------------
#[derive(Debug, Clone, Serialize)]
pub struct StockBoard {
    pub name: String,
    pub code: String,
    pub kind: String, // 行业 / 板块（地域） / 概念
}

/// 指数成分、持仓风格类"伪概念"黑名单（子串命中即丢——既不展示也不进 AI 上下文）
const BOARD_NOISE: &[&str] = &[
    "标准普尔", "富时罗素", "MSCI", "沪股通", "深股通", "融资融券", "转融券",
    "上证50", "上证180", "上证380", "央视50", "HS300", "中证500", "深成500",
    "百元股", "高价股", "低价股", "大盘股", "中盘股", "小盘股", "微盘股", "权重股",
    "机构重仓", "基金重仓", "社保重仓", "QFII", "证金持股", "保险重仓", "信托重仓",
    "茅指数", "宁组合", "超级品牌", "东方财富热股", "行业龙头", "昨日", "次新股",
    "AB股", "AH股", "B股", "GDR", "破净股", "风格", "样本股", "成份", "标的",
    "创业板综", "深证100",
];

/// 解析东财 F10 所属板块（2026-06-12 实测）：
/// result.data[] = {"BOARD_NAME":"白酒","BOARD_CODE":"896","BOARD_TYPE":null|"行业"|"板块"}
/// BOARD_TYPE：行业=东财行业分类，板块=地域板块，null=概念/题材/指数成分。
/// 处理：黑名单去噪 → 细分行业Ⅱ/Ⅲ去重 → 行业排最前 → 同名去重 → 截前 10 个。
pub fn parse_stock_boards(body: &str) -> Vec<StockBoard> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match v["result"]["data"].as_array() {
        Some(a) => a,
        None => return vec![],
    };
    let mut seen = std::collections::HashSet::new();
    let mut industry: Vec<StockBoard> = vec![];
    let mut concept: Vec<StockBoard> = vec![];
    for d in arr {
        let raw = match d["BOARD_NAME"].as_str() {
            Some(n) if !n.is_empty() => n,
            _ => continue,
        };
        if BOARD_NOISE.iter().any(|w| raw.contains(w)) {
            continue;
        }
        if raw.ends_with('Ⅱ') || raw.ends_with('Ⅲ') {
            continue; // 细分行业（白酒Ⅱ/Ⅲ）与一级重复
        }
        let name = raw.trim_end_matches('_').to_string();
        if !seen.insert(name.clone()) {
            continue;
        }
        let kind = d["BOARD_TYPE"].as_str().unwrap_or("");
        let b = StockBoard {
            name,
            code: d["BOARD_CODE"].as_str().unwrap_or("").to_string(),
            kind: if kind.is_empty() { "概念".into() } else { kind.to_string() },
        };
        if kind == "行业" {
            industry.push(b);
        } else {
            concept.push(b);
        }
    }
    industry.extend(concept);
    industry.truncate(10);
    industry
}

#[cfg(feature = "net")]
pub async fn fetch_stock_boards(code: &str) -> Result<Vec<StockBoard>, String> {
    let lc = code.to_lowercase();
    let suffix = if lc.starts_with("sh") && lc.len() == 8 {
        "SH"
    } else if lc.starts_with("sz") && lc.len() == 8 {
        "SZ"
    } else {
        return Err(format!("无法识别代码: {code}"));
    };
    let six = &lc[2..];
    let url = format!(
        "https://datacenter.eastmoney.com/securities/api/data/v1/get?reportName=RPT_F10_CORETHEME_BOARDTYPE\
         &columns=SECUCODE,SECURITY_CODE,BOARD_NAME,BOARD_CODE,BOARD_TYPE\
         &filter=(SECUCODE%3D%22{six}.{suffix}%22)&client=WEB&source=HSF10&pageSize=60"
    );
    let text = em_get_text(&url, "https://emweb.securities.eastmoney.com/")
        .await
        .map_err(|e| format!("所属板块请求失败: {e}"))?;
    let list = parse_stock_boards(&text);
    if list.is_empty() {
        return Err("所属板块解析为空".into());
    }
    Ok(list)
}

// ------------------------------------------------------------
// 北向资金（沪深港通）
// ⚠️ 2024-08 交易所新规后北向"净买额"已停止披露：
//   · push2 kamt / kamt.rtmin 实时接口北向字段恒为 0 / "-"（2026-06-12 复核）
//   · RPT_MUTUAL_DEAL_HISTORY 的 NET_DEAL_AMT / FUND_INFLOW 为 null
// 仍按日披露的只有"成交金额"（次日更新）→ 只返回成交额，绝不伪造净流入。
// ------------------------------------------------------------
#[derive(Debug, Clone, Serialize)]
pub struct NorthDay {
    pub date: String, // YYYY-MM-DD
    pub hu: f64,      // 沪股通成交金额（亿元）
    pub sz: f64,      // 深股通成交金额（亿元）
}

/// 解析沪深港通历史单边返回（2026-06-12 实测）：
/// result.data[] = {"TRADE_DATE":"2026-06-11 00:00:00","DEAL_AMT":159596.74,…}
/// DEAL_AMT 单位百万元（与 akshare 对该报表的口径一致）→ /100 转亿元；null 行剔除。
pub fn parse_north_history(body: &str) -> Vec<(String, f64)> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match v["result"]["data"].as_array() {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter()
        .filter_map(|d| {
            let date = d["TRADE_DATE"].as_str()?.get(..10)?.to_string();
            let amt = d["DEAL_AMT"].as_f64()?; // null（未披露/未更新）→ 剔除
            Some((date, amt / 100.0))
        })
        .collect()
}

/// 拉沪深港通历史一边（001=沪股通 003=深股通），失败返回空（由调用方合并判空）
#[cfg(feature = "net")]
async fn fetch_mutual_history(mutual_type: &str) -> Vec<(String, f64)> {
    let url = format!(
        "https://datacenter-web.eastmoney.com/api/data/v1/get?sortColumns=TRADE_DATE&sortTypes=-1\
         &pageSize=5&pageNumber=1&reportName=RPT_MUTUAL_DEAL_HISTORY&columns=ALL&source=WEB&client=WEB\
         &filter=(MUTUAL_TYPE%3D%22{mutual_type}%22)"
    );
    match em_get_text(&url, "https://data.eastmoney.com/").await {
        Ok(t) => parse_north_history(&t),
        Err(_) => vec![],
    }
}

/// 北向（沪+深股通）近 5 个交易日成交金额（亿元），最新交易日在前
#[cfg(feature = "net")]
pub async fn fetch_north_flow() -> Result<Vec<NorthDay>, String> {
    let (hu, sz) = tokio::join!(fetch_mutual_history("001"), fetch_mutual_history("003"));
    let mut map: std::collections::BTreeMap<String, NorthDay> = Default::default();
    for (date, amt) in hu {
        map.entry(date.clone())
            .or_insert_with(|| NorthDay { date: date.clone(), hu: 0.0, sz: 0.0 })
            .hu = amt;
    }
    for (date, amt) in sz {
        map.entry(date.clone())
            .or_insert_with(|| NorthDay { date: date.clone(), hu: 0.0, sz: 0.0 })
            .sz = amt;
    }
    let mut out: Vec<NorthDay> = map.into_values().collect();
    out.sort_by(|a, b| b.date.cmp(&a.date)); // 最新在前
    out.truncate(5);
    if out.is_empty() {
        return Err("沪深港通历史成交为空".into());
    }
    Ok(out)
}

#[cfg(test)]
mod dim_tests {
    use super::*;

    #[test]
    fn test_parse_flow_days() {
        // 2026-06-12 实测真实形状（贵州茅台，截取 2 行）
        let raw = r#"{"rc":0,"data":{"code":"600519","market":1,"name":"贵州茅台","klines":[
            "2026-06-10,268756032.0,-468265.0,-268287760.0,-55750576.0,324506608.0,5.38,-0.01,-5.37,-1.12,6.50,1275.88,1.58,0.00,0.00",
            "2026-06-11,-95013600.0,-81509.0,95095120.0,-80124064.0,-14889536.0,-2.94,-0.00,2.94,-2.48,-0.46,1279.00,0.24,0.00,0.00"]}}"#;
        let l = parse_flow_days(raw);
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].date, "2026-06-11");
        assert!((l[1].main - (-95013600.0)).abs() < 1.0);
        assert!((l[1].main_pct - (-2.94)).abs() < 0.01);
        assert!((l[1].close - 1279.0).abs() < 0.01);
        assert!((l[1].change_pct - 0.24).abs() < 0.01);
        assert!(parse_flow_days("x").is_empty());
        assert!(parse_flow_days(r#"{"data":{"klines":[]}}"#).is_empty());
    }

    #[test]
    fn test_parse_stock_boards() {
        // 2026-06-12 实测真实形状（600519 截取）：行业最前；指数成分/两融/细分Ⅲ滤掉
        let raw = r#"{"result":{"data":[
            {"BOARD_NAME":"茅指数","BOARD_CODE":"999","BOARD_TYPE":null},
            {"BOARD_NAME":"白酒","BOARD_CODE":"896","BOARD_TYPE":null},
            {"BOARD_NAME":"标准普尔","BOARD_CODE":"879","BOARD_TYPE":null},
            {"BOARD_NAME":"食品饮料","BOARD_CODE":"438","BOARD_TYPE":"行业"},
            {"BOARD_NAME":"白酒Ⅲ","BOARD_CODE":"1575","BOARD_TYPE":"行业"},
            {"BOARD_NAME":"贵州板块","BOARD_CODE":"173","BOARD_TYPE":"板块"},
            {"BOARD_NAME":"融资融券","BOARD_CODE":"596","BOARD_TYPE":null}
        ]},"success":true}"#;
        let b = parse_stock_boards(raw);
        let names: Vec<&str> = b.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["食品饮料", "白酒", "贵州板块"]);
        assert_eq!(b[0].kind, "行业");
        assert_eq!(b[1].kind, "概念");
        assert_eq!(b[2].kind, "板块");
        assert!(parse_stock_boards("x").is_empty());
    }

    #[test]
    fn test_parse_north_history() {
        // 2026-06-12 实测真实形状（沪股通截取；净买额字段已是 null）
        let raw = r#"{"result":{"data":[
            {"MUTUAL_TYPE":"001","TRADE_DATE":"2026-06-11 00:00:00","NET_DEAL_AMT":null,"FUND_INFLOW":null,"DEAL_AMT":159596.74},
            {"MUTUAL_TYPE":"001","TRADE_DATE":"2026-06-10 00:00:00","NET_DEAL_AMT":null,"FUND_INFLOW":null,"DEAL_AMT":166385.91},
            {"MUTUAL_TYPE":"001","TRADE_DATE":"2026-06-09 00:00:00","NET_DEAL_AMT":null,"FUND_INFLOW":null,"DEAL_AMT":null}
        ]},"success":true}"#;
        let l = parse_north_history(raw);
        assert_eq!(l.len(), 2); // DEAL_AMT null 行剔除
        assert_eq!(l[0].0, "2026-06-11");
        assert!((l[0].1 - 1595.9674).abs() < 0.01); // 百万元 → 亿元
        assert!(parse_north_history("x").is_empty());
    }
}

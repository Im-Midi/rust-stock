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

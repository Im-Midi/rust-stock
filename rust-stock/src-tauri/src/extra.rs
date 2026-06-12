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

// ============================================================
// 第五批三个 A 股数据维度（接口设计参考 1nchaos/adata 接口字典，Apache-2.0，
// 未复制其代码；全部接口 2026-06-12 经真实请求复核形状）：
//   ① 人气榜（同花顺热度榜，普通 GET，code+name+涨跌幅一次到位）
//   ② 分红历史（东财 datacenter RPT_SHAREBONUS_DET）
//   ③ 股本/市值（东财 push2 stock/get f84/f85/f116/f117 + f162/f167）
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct HotStock {
    pub rank: u32,        // 名次（order）
    pub code: String,     // 统一格式 sh600105
    pub name: String,
    pub change_pct: f64,  // 涨跌幅 %（rise_and_fall）
    pub hot: f64,         // 人气值（rate，字符串数值）
    pub tag: String,      // 人气标签（如"3天2板"），无则取首个概念名
}

/// 解析同花顺热度榜（2026-06-12 实测）：
/// data.stock_list[] = {"market":17|33,"code":"600105","name":"永鼎股份",
///   "rate":"884418.0","rise_and_fall":9.9885,"order":1,
///   "tag":{"concept_tag":["共封装光学(CPO)"],"popularity_tag":"3天2板"}}
/// market：17=沪 33=深；其余（北交所等）丢弃，与全 app 仅支持 sh/sz 口径一致。
pub fn parse_hot_stocks(body: &str) -> Vec<HotStock> {
    let v: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match v["data"]["stock_list"].as_array() {
        Some(a) => a,
        None => return vec![],
    };
    arr.iter()
        .filter_map(|d| {
            let prefix = match d["market"].as_i64()? {
                17 => "sh",
                33 => "sz",
                _ => return None, // 北交所等
            };
            let code6 = d["code"].as_str()?;
            if code6.len() != 6 {
                return None;
            }
            let name = d["name"].as_str().unwrap_or("").to_string();
            if name.is_empty() {
                return None;
            }
            let tag = d["tag"]["popularity_tag"]
                .as_str()
                .map(|s| s.to_string())
                .or_else(|| {
                    d["tag"]["concept_tag"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            Some(HotStock {
                rank: d["order"].as_u64().unwrap_or(0) as u32,
                code: format!("{prefix}{code6}"),
                name,
                change_pct: num(&d["rise_and_fall"]),
                hot: num(&d["rate"]),
                tag,
            })
        })
        .collect()
}

/// 同花顺 A 股人气榜（小时榜）Top 15。普通 GET、免 Cookie/签名，
/// 返回即含 code+name+涨跌幅，无需二次行情解析（东财人气榜 POST 只回 secid，弃用）。
#[cfg(feature = "net")]
pub async fn fetch_hot_stocks() -> Result<Vec<HotStock>, String> {
    let url = "https://dq.10jqka.com.cn/fuyao/hot_list_data/out/hot_list/v1/stock?stock_type=a&type=hour&list_type=normal";
    let text = em_get_text(url, "https://eq.10jqka.com.cn/")
        .await
        .map_err(|e| format!("人气榜请求失败: {e}"))?;
    let mut list = parse_hot_stocks(&text);
    if list.is_empty() {
        return Err("人气榜解析为空".into());
    }
    list.truncate(15);
    Ok(list)
}

// ------------------------------------------------------------
// 分红历史（东财 datacenter RPT_SHAREBONUS_DET）
// ------------------------------------------------------------
#[derive(Debug, Clone, Serialize)]
pub struct Dividend {
    pub plan: String,        // 方案文本，如 "10派239.57元" / "10转6.00"（去含税附注）
    pub ex_date: String,     // 除权除息日 YYYY-MM-DD（未实施为空）
    pub record_date: String, // 股权登记日 YYYY-MM-DD（可空）
    pub progress: String,    // 实施分配 / 预案 …
    pub yield_pct: f64,      // 股息率 %（公告时点 DIVIDENT_RATIO×100；未披露为 0）
    pub report_date: String, // 对应报告期 YYYY-MM-DD
}

/// 解析东财分红明细（2026-06-12 实测）：result.data[]，关键字段
/// IMPL_PLAN_PROFILE("10派239.57元(含税,扣税后215.613元)" / "10转6.00")、
/// EX_DIVIDEND_DATE / EQUITY_RECORD_DATE（"YYYY-MM-DD 00:00:00"，未实施可为 null）、
/// ASSIGN_PROGRESS、DIVIDENT_RATIO(0.0167=1.67%，纯转股为 null)、REPORT_DATE。
/// 返回 None=响应无效；Some(vec![])=该股确实无分红记录（result 为 null）。
pub fn parse_dividends(body: &str) -> Option<Vec<Dividend>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v["success"] != serde_json::Value::Bool(true) {
        return None;
    }
    let arr = match v["result"]["data"].as_array() {
        Some(a) => a,
        None => return Some(vec![]), // success 但 result=null → 无分红记录
    };
    let date10 = |val: &serde_json::Value| -> String {
        val.as_str().and_then(|s| s.get(..10)).unwrap_or("").to_string()
    };
    Some(
        arr.iter()
            .filter_map(|d| {
                let raw = d["IMPL_PLAN_PROFILE"].as_str()?;
                if raw.is_empty() || raw.contains("不分配") {
                    return None; // "不分配不转增"等非分红行
                }
                let plan = raw.split('(').next().unwrap_or(raw).trim().to_string();
                Some(Dividend {
                    plan,
                    ex_date: date10(&d["EX_DIVIDEND_DATE"]),
                    record_date: date10(&d["EQUITY_RECORD_DATE"]),
                    progress: d["ASSIGN_PROGRESS"].as_str().unwrap_or("").to_string(),
                    yield_pct: num(&d["DIVIDENT_RATIO"]) * 100.0,
                    report_date: date10(&d["REPORT_DATE"]),
                })
            })
            .collect(),
    )
}

/// 个股分红历史（最近 8 期，按预案公告日倒序=最新方案在前）。
/// Ok(空数组) = 该股确实从未分红（前端隐藏该行，不算失败）。
#[cfg(feature = "net")]
pub async fn fetch_dividends(code: &str) -> Result<Vec<Dividend>, String> {
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
        "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPT_SHAREBONUS_DET\
         &columns=ALL&filter=(SECUCODE%3D%22{six}.{suffix}%22)&pageNumber=1&pageSize=8\
         &sortColumns=PLAN_NOTICE_DATE&sortTypes=-1&source=WEB&client=WEB"
    );
    let text = em_get_text(&url, "https://data.eastmoney.com/")
        .await
        .map_err(|e| format!("分红请求失败: {e}"))?;
    parse_dividends(&text).ok_or_else(|| "分红响应无效".to_string())
}

// ------------------------------------------------------------
// 股本 / 市值（东财 push2 stock/get；fltt=2 → 数值不带 ×100 倍率）
// ------------------------------------------------------------
#[derive(Debug, Clone, Serialize)]
pub struct ShareInfo {
    pub total_shares: f64, // 总股本（股）f84
    pub float_shares: f64, // 流通股（股）f85
    pub total_cap: f64,    // 总市值（元）f116
    pub float_cap: f64,    // 流通市值（元）f117
    pub pe: f64,           // 市盈率(动) f162（亏损时可为负/缺省 0）
    pub pb: f64,           // 市净率 f167
}

/// 解析东财 push2 stock/get（2026-06-12 实测，fltt=2）：
/// {"data":{"f43":1291.91,"f57":"600519","f84":1250081601.0,"f85":1250081601.0,
///   "f116":1614992921147.9102,"f117":1614992921147.9102,"f162":14.82,"f167":5.96}}
/// f116 == f43×f84、f117 == f43×f85（实测核对）；缺省值 "-" 由 num() 归 0。
pub fn parse_share_info(body: &str) -> Option<ShareInfo> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let d = v.get("data")?;
    if !d.is_object() {
        return None;
    }
    let info = ShareInfo {
        total_shares: num(&d["f84"]),
        float_shares: num(&d["f85"]),
        total_cap: num(&d["f116"]),
        float_cap: num(&d["f117"]),
        pe: num(&d["f162"]),
        pb: num(&d["f167"]),
    };
    if info.total_shares <= 0.0 && info.total_cap <= 0.0 {
        return None; // 全空响应不算成功
    }
    Some(info)
}

#[cfg(feature = "net")]
pub async fn fetch_share_info(code: &str) -> Result<ShareInfo, String> {
    let secid = crate::sources::to_secid(code).ok_or_else(|| format!("无法识别代码: {code}"))?;
    // 注意：f84/f85/f116/f117 是 stock/get 的字段口径；ulist/clist 同编号含义不同
    //（ulist 的 f84 是小单净额），不能合并进 fetch_fund_flow 的请求。
    let url = format!(
        "https://push2.eastmoney.com/api/qt/stock/get?secid={secid}\
         &fields=f84,f85,f116,f117,f162,f167&fltt=2&invt=2"
    );
    let text = em_get_text(&url, "https://quote.eastmoney.com/")
        .await
        .map_err(|e| format!("股本市值请求失败: {e}"))?;
    parse_share_info(&text).ok_or_else(|| "股本市值解析为空".to_string())
}

#[cfg(test)]
mod dim5_tests {
    use super::*;

    #[test]
    fn test_parse_hot_stocks() {
        // 2026-06-12 实测真实形状（截取：沪、深、北交所各一）
        let raw = r#"{"status_code":0,"data":{"stock_list":[
            {"market":17,"code":"600105","rate":"884418.0","rise_and_fall":9.9885,"name":"永鼎股份","hot_rank_chg":0,"tag":{"concept_tag":["共封装光学(CPO)","光纤概念"],"popularity_tag":"3天2板"},"order":1},
            {"market":33,"code":"002580","rate":"732816.0","rise_and_fall":-10.0290,"name":"圣阳股份","tag":{"concept_tag":["高铁"]},"order":2},
            {"market":151,"code":"430047","rate":"10.0","rise_and_fall":1.0,"name":"诺思兰德","tag":{},"order":3}
        ]},"status_msg":"success"}"#;
        let l = parse_hot_stocks(raw);
        assert_eq!(l.len(), 2); // 北交所丢弃
        assert_eq!(l[0].code, "sh600105");
        assert_eq!(l[0].rank, 1);
        assert_eq!(l[0].tag, "3天2板"); // popularity_tag 优先
        assert!((l[0].change_pct - 9.9885).abs() < 1e-4);
        assert!((l[0].hot - 884418.0).abs() < 0.1); // rate 是字符串数值
        assert_eq!(l[1].code, "sz002580");
        assert_eq!(l[1].tag, "高铁"); // 无 popularity_tag → 首个概念
        assert!(l[1].change_pct < 0.0);
        assert!(parse_hot_stocks("x").is_empty());
        assert!(parse_hot_stocks(r#"{"data":{}}"#).is_empty());
    }

    #[test]
    fn test_parse_dividends() {
        // 2026-06-12 实测真实形状（贵州茅台派现 + 奥瑞德纯转股 DIVIDENT_RATIO=null）
        let raw = r#"{"result":{"data":[
            {"IMPL_PLAN_PROFILE":"10派239.57元(含税,扣税后215.613元)","EX_DIVIDEND_DATE":"2025-12-19 00:00:00","EQUITY_RECORD_DATE":"2025-12-18 00:00:00","ASSIGN_PROGRESS":"实施分配","DIVIDENT_RATIO":0.016741439553,"REPORT_DATE":"2025-09-30 00:00:00"},
            {"IMPL_PLAN_PROFILE":"10转6.00","EX_DIVIDEND_DATE":"2017-05-26 00:00:00","EQUITY_RECORD_DATE":"2017-05-25 00:00:00","ASSIGN_PROGRESS":"实施分配","DIVIDENT_RATIO":null,"REPORT_DATE":"2016-12-31 00:00:00"},
            {"IMPL_PLAN_PROFILE":"不分配不转增","EX_DIVIDEND_DATE":null,"EQUITY_RECORD_DATE":null,"ASSIGN_PROGRESS":"董事会预案","DIVIDENT_RATIO":null,"REPORT_DATE":"2020-12-31 00:00:00"}
        ],"count":3},"success":true,"message":"ok","code":0}"#;
        let l = parse_dividends(raw).unwrap();
        assert_eq!(l.len(), 2); // "不分配"行剔除
        assert_eq!(l[0].plan, "10派239.57元"); // 含税附注剥离
        assert_eq!(l[0].ex_date, "2025-12-19");
        assert_eq!(l[0].record_date, "2025-12-18");
        assert!((l[0].yield_pct - 1.6741439553).abs() < 1e-6);
        assert_eq!(l[1].plan, "10转6.00");
        assert!(l[1].yield_pct.abs() < 1e-9); // null → 0
        // success 但 result=null → 确实无分红（Some(空)），与解析失败（None）区分
        assert_eq!(parse_dividends(r#"{"result":null,"success":true}"#).unwrap().len(), 0);
        assert!(parse_dividends(r#"{"success":false}"#).is_none());
        assert!(parse_dividends("x").is_none());
    }

    #[test]
    fn test_parse_share_info() {
        // 2026-06-12 实测真实形状（贵州茅台，fltt=2）
        let raw = r#"{"rc":0,"rt":4,"data":{"f43":1291.91,"f57":"600519","f58":"贵州茅台","f84":1250081601.0,"f85":1250081601.0,"f116":1614992921147.9102,"f117":1614992921147.9102,"f162":14.82,"f167":5.96}}"#;
        let s = parse_share_info(raw).unwrap();
        assert!((s.total_shares - 1250081601.0).abs() < 1.0);
        assert!((s.float_shares - 1250081601.0).abs() < 1.0);
        assert!((s.total_cap - 1614992921147.91).abs() < 1.0);
        assert!((s.pe - 14.82).abs() < 0.01);
        assert!((s.pb - 5.96).abs() < 0.01);
        // 市值 = 现价 × 股本（口径核对）
        assert!((s.total_cap - 1291.91 * s.total_shares).abs() / s.total_cap < 1e-6);
        assert!(parse_share_info(r#"{"data":null}"#).is_none());
        assert!(parse_share_info(r#"{"data":{"f84":"-","f116":"-"}}"#).is_none());
        assert!(parse_share_info("x").is_none());
    }
}


// ============================================================
// 第六批三个 A 股数据维度（接口设计参考 1nchaos/adata 接口字典，Apache-2.0，
// 未复制其代码；全部接口 2026-06-12 经真实请求复核形状）：
//   ① 核心财务指标（东财 F10 RPT_F10_FINANCE_MAINFINADATA，主要财务指标）
//   ② 融资融券个股余额（东财 datacenter RPTA_WEB_RZRQ_GGMX；
//      注意 adata 字典里的 RPT_RZRQ_LSHDETAIL 在 web datacenter 已不存在，
//      实测返回"报表配置不存在"——个股两融明细的现行报表是 RPTA_WEB_RZRQ_GGMX）
//   ③ 个股龙虎榜上榜明细（RPT_DAILYBILLBOARD_DETAILSNEW 按 SECURITY_CODE 过滤）
// ============================================================

/// null / "-" 安全取数：缺失返回 None（serde 序列化为 JSON null，
/// 前端显示 "--" 或整段省略——绝不把"未披露"伪装成 0）
fn optnum(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// 把统一代码 sh600519 / sz000001 拆成 (六位码, "SH"/"SZ")
fn split_code(code: &str) -> Result<(String, &'static str), String> {
    let lc = code.to_lowercase();
    let suffix = if lc.starts_with("sh") && lc.len() == 8 {
        "SH"
    } else if lc.starts_with("sz") && lc.len() == 8 {
        "SZ"
    } else {
        return Err(format!("无法识别代码: {code}"));
    };
    Ok((lc[2..].to_string(), suffix))
}

/// Hinnant days_from_civil：公历 (y,m,d) → 距 1970-01-01 的天数（纯函数，免依赖 chrono）
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0,399]
    let mp = ((m + 9) % 12) as i64; // 三月起算月序 [0,11]
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// "YYYY-MM-DD" → 距今天数（UTC 日界；±1 天误差对"近 30 日"判断无影响）。
/// 解析失败返回 None。供后端按日期过滤龙虎榜等"仅近期才注入 AI"的数据。
pub(crate) fn days_since(date: &str) -> Option<i64> {
    let mut it = date.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let now_days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64
        / 86400;
    Some(now_days - days_from_civil(y, m, d))
}

// ------------------------------------------------------------
// ① 核心财务指标（最新报告期一行）
// ------------------------------------------------------------
#[derive(Debug, Clone, Serialize)]
pub struct Financials {
    pub report: String,              // 报告期名，如 "2026一季报"（REPORT_DATE_NAME）
    pub report_date: String,         // 报告期 YYYY-MM-DD（REPORT_DATE）
    pub revenue: Option<f64>,        // 营业总收入（元）TOTALOPERATEREVE
    pub revenue_yoy: Option<f64>,    // 营收同比 %  TOTALOPERATEREVETZ
    pub net_profit: Option<f64>,     // 归母净利润（元）PARENTNETPROFIT
    pub net_profit_yoy: Option<f64>, // 净利同比 %  PARENTNETPROFITTZ
    pub eps: Option<f64>,            // 基本每股收益（元）EPSJB
    pub roe: Option<f64>,            // 加权净资产收益率 % ROEJQ
    pub gross_margin: Option<f64>,   // 销售毛利率 % XSMLL（银行/券商为 null）
    pub debt_ratio: Option<f64>,     // 资产负债率 % ZCFZL
}

/// 解析东财 F10 主要财务指标（2026-06-12 实测）：result.data[]（按 REPORT_DATE 倒序，
/// 取第一行=最新报告期）。关键字段名见结构体注释；金额单位为元、比率已是百分数。
/// None=响应无效或该股无任何财报数据（前端隐藏区块）。
pub fn parse_financials(body: &str) -> Option<Financials> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v["success"] != serde_json::Value::Bool(true) {
        return None;
    }
    let d = v["result"]["data"].as_array()?.first()?;
    let report = d["REPORT_DATE_NAME"].as_str().unwrap_or("").to_string();
    let report_date = d["REPORT_DATE"]
        .as_str()
        .and_then(|s| s.get(..10))
        .unwrap_or("")
        .to_string();
    if report.is_empty() && report_date.is_empty() {
        return None;
    }
    let f = Financials {
        report,
        report_date,
        revenue: optnum(&d["TOTALOPERATEREVE"]),
        revenue_yoy: optnum(&d["TOTALOPERATEREVETZ"]),
        net_profit: optnum(&d["PARENTNETPROFIT"]),
        net_profit_yoy: optnum(&d["PARENTNETPROFITTZ"]),
        eps: optnum(&d["EPSJB"]),
        roe: optnum(&d["ROEJQ"]),
        gross_margin: optnum(&d["XSMLL"]),
        debt_ratio: optnum(&d["ZCFZL"]),
    };
    if f.revenue.is_none() && f.net_profit.is_none() {
        return None; // 关键数字全缺 → 视为无有效财报
    }
    Some(f)
}

#[cfg(feature = "net")]
pub async fn fetch_financials(code: &str) -> Result<Financials, String> {
    let (six, suffix) = split_code(code)?;
    // 与 fetch_stock_boards 同一 F10 datacenter；columns=ALL 即实测复核过的完整形状
    let url = format!(
        "https://datacenter.eastmoney.com/securities/api/data/v1/get?reportName=RPT_F10_FINANCE_MAINFINADATA\
         &columns=ALL&filter=(SECUCODE%3D%22{six}.{suffix}%22)&client=WEB&source=HSF10\
         &pageSize=1&sortColumns=REPORT_DATE&sortTypes=-1"
    );
    let text = em_get_text(&url, "https://emweb.securities.eastmoney.com/")
        .await
        .map_err(|e| format!("财务指标请求失败: {e}"))?;
    parse_financials(&text).ok_or_else(|| "财务指标解析为空".to_string())
}

// ------------------------------------------------------------
// ② 融资融券个股余额（最近 2 个交易日，最新在前 → 可算较上日增减）
// ------------------------------------------------------------
#[derive(Debug, Clone, Serialize)]
pub struct MarginDay {
    pub date: String,   // 交易日 YYYY-MM-DD（DATE）
    pub rzye: f64,      // 融资余额（元）RZYE
    pub rqye: f64,      // 融券余额（元）RQYE
    pub rzrqye: f64,    // 融资融券余额（元）RZRQYE
    pub rzjme: f64,     // 当日融资净买额（元，买入-偿还）RZJME
}

/// 解析东财个股两融明细（2026-06-12 实测）：result.data[] 按 date 倒序。
/// 返回 None=响应无效；Some(vec![])=非两融标的——datacenter 对查无数据
/// 返回 {"success":false,"code":9201,"message":"返回数据为空"}（实测），
/// 与真正的请求失败（网络/解析错误）严格区分。
pub fn parse_margin_days(body: &str) -> Option<Vec<MarginDay>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v["success"] != serde_json::Value::Bool(true) {
        // 9201 = 查询成功但无记录（非两融标的）；其余按无效响应处理
        return if v["code"].as_i64() == Some(9201) { Some(vec![]) } else { None };
    }
    let arr = match v["result"]["data"].as_array() {
        Some(a) => a,
        None => return Some(vec![]),
    };
    Some(
        arr.iter()
            .filter_map(|d| {
                let date = d["DATE"].as_str()?.get(..10)?.to_string();
                Some(MarginDay {
                    date,
                    rzye: num(&d["RZYE"]),
                    rqye: num(&d["RQYE"]),
                    rzrqye: num(&d["RZRQYE"]),
                    rzjme: num(&d["RZJME"]),
                })
            })
            .collect(),
    )
}

/// 个股两融余额（最近 2 个交易日，最新在前）。Ok(空数组)=非两融标的（前端隐藏该行）
#[cfg(feature = "net")]
pub async fn fetch_margin(code: &str) -> Result<Vec<MarginDay>, String> {
    let (six, _) = split_code(code)?;
    let url = format!(
        "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPTA_WEB_RZRQ_GGMX\
         &columns=ALL&filter=(scode%3D%22{six}%22)&pageNumber=1&pageSize=2\
         &sortColumns=date&sortTypes=-1&source=WEB&client=WEB"
    );
    let text = em_get_text(&url, "https://data.eastmoney.com/")
        .await
        .map_err(|e| format!("两融请求失败: {e}"))?;
    parse_margin_days(&text).ok_or_else(|| "两融响应无效".to_string())
}

// ------------------------------------------------------------
// ③ 个股龙虎榜上榜明细（最近 5 次，最新在前；"是否最近上榜"由前端按日期判断）
// ------------------------------------------------------------
#[derive(Debug, Clone, Serialize)]
pub struct LhbItem {
    pub date: String,    // 上榜日 YYYY-MM-DD（TRADE_DATE）
    pub reason: String,  // 上榜原因（EXPLANATION）
    pub net: f64,        // 龙虎榜净买额（元）BILLBOARD_NET_AMT
    pub buy: f64,        // 龙虎榜买入额（元）BILLBOARD_BUY_AMT
    pub sell: f64,       // 龙虎榜卖出额（元）BILLBOARD_SELL_AMT
}

/// 解析个股龙虎榜明细（2026-06-12 实测；同日多原因上榜会有多行，全保留）。
/// None=响应无效；Some(vec![])=从未上榜（datacenter 同样回 9201"返回数据为空"）。
pub fn parse_lhb_detail(body: &str) -> Option<Vec<LhbItem>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    if v["success"] != serde_json::Value::Bool(true) {
        return if v["code"].as_i64() == Some(9201) { Some(vec![]) } else { None };
    }
    let arr = match v["result"]["data"].as_array() {
        Some(a) => a,
        None => return Some(vec![]),
    };
    Some(
        arr.iter()
            .filter_map(|d| {
                let date = d["TRADE_DATE"].as_str()?.get(..10)?.to_string();
                Some(LhbItem {
                    date,
                    reason: d["EXPLANATION"].as_str().unwrap_or("").to_string(),
                    net: num(&d["BILLBOARD_NET_AMT"]),
                    buy: num(&d["BILLBOARD_BUY_AMT"]),
                    sell: num(&d["BILLBOARD_SELL_AMT"]),
                })
            })
            .collect(),
    )
}

/// 个股龙虎榜历史（最近 5 次，最新在前）。Ok(空数组)=从未上榜（前端隐藏该行）；
/// 是否"近期"由前端按本机日期过滤（≤30 天才展示/注入 AI），后端不做日期运算。
#[cfg(feature = "net")]
pub async fn fetch_lhb_detail(code: &str) -> Result<Vec<LhbItem>, String> {
    let (six, _) = split_code(code)?;
    let url = format!(
        "https://datacenter-web.eastmoney.com/api/data/v1/get?reportName=RPT_DAILYBILLBOARD_DETAILSNEW\
         &columns=ALL&filter=(SECURITY_CODE%3D%22{six}%22)&pageNumber=1&pageSize=5\
         &sortColumns=TRADE_DATE&sortTypes=-1&source=WEB&client=WEB"
    );
    let text = em_get_text(&url, "https://data.eastmoney.com/")
        .await
        .map_err(|e| format!("龙虎榜请求失败: {e}"))?;
    parse_lhb_detail(&text).ok_or_else(|| "龙虎榜响应无效".to_string())
}

#[cfg(test)]
mod dim6_tests {
    use super::*;

    #[test]
    fn test_days_from_civil() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2000, 1, 1), 10957);
        assert_eq!(days_from_civil(2026, 6, 12), 20616);
        // days_since：今天≈0、明显过去为大正数、非法输入 None
        assert!(days_since("1970-01-01").unwrap() > 20000);
        assert!(days_since("废话").is_none());
        assert!(days_since("2026-13-01").is_none());
    }

    #[test]
    fn test_parse_financials() {
        // 2026-06-12 实测真实形状（贵州茅台 2026 一季报，截取关键字段；EPSKCJB 为 null）
        let raw = r#"{"result":{"data":[
            {"SECUCODE":"600519.SH","REPORT_DATE":"2026-03-31 00:00:00","REPORT_DATE_NAME":"2026一季报",
             "EPSJB":21.76,"EPSKCJB":null,"TOTALOPERATEREVE":54702912385.23,"TOTALOPERATEREVETZ":6.336009277123,
             "PARENTNETPROFIT":27242512886.45,"PARENTNETPROFITTZ":1.471418294983,
             "ROEJQ":10.57,"XSMLL":89.7592176242,"ZCFZL":12.1227489682}
        ],"count":102},"success":true,"message":"ok","code":0}"#;
        let f = parse_financials(raw).unwrap();
        assert_eq!(f.report, "2026一季报");
        assert_eq!(f.report_date, "2026-03-31");
        assert!((f.revenue.unwrap() - 54702912385.23).abs() < 1.0);
        assert!((f.revenue_yoy.unwrap() - 6.336009277123).abs() < 1e-6);
        assert!((f.net_profit.unwrap() - 27242512886.45).abs() < 1.0);
        assert!((f.net_profit_yoy.unwrap() - 1.471418294983).abs() < 1e-6);
        assert!((f.eps.unwrap() - 21.76).abs() < 1e-6);
        assert!((f.roe.unwrap() - 10.57).abs() < 1e-6);
        assert!((f.gross_margin.unwrap() - 89.7592176242).abs() < 1e-6);
        assert!((f.debt_ratio.unwrap() - 12.1227489682).abs() < 1e-6);
        // 银行类：毛利率为 null → None（不造 0）
        let bank = r#"{"result":{"data":[{"REPORT_DATE":"2026-03-31 00:00:00","REPORT_DATE_NAME":"2026一季报",
            "TOTALOPERATEREVE":1.0,"XSMLL":null}]},"success":true}"#;
        let b = parse_financials(bank).unwrap();
        assert!(b.gross_margin.is_none());
        assert!(b.net_profit.is_none());
        // 关键数字全缺 / 响应无效 / 无数据 → None
        assert!(parse_financials(r#"{"result":{"data":[{"REPORT_DATE_NAME":"2026一季报"}]},"success":true}"#).is_none());
        assert!(parse_financials(r#"{"result":null,"success":false,"code":9201}"#).is_none());
        assert!(parse_financials("x").is_none());
    }

    #[test]
    fn test_parse_margin_days() {
        // 2026-06-12 实测真实形状（贵州茅台，截取 2 日；RZRQYE 可带小数）
        let raw = r#"{"result":{"pages":1305,"data":[
            {"DATE":"2026-06-11 00:00:00","SCODE":"600519","RZYE":19190663674,"RQYL":133635,
             "RZRQYE":19361582839,"RQYE":170919165,"RZJME":-68727865,"SPJ":1279,"ZDF":0.2445},
            {"DATE":"2026-06-10 00:00:00","SCODE":"600519","RZYE":19259391539,"RQYL":140435,
             "RZRQYE":19438569746.8,"RQYE":179178207.8,"RZJME":-158389565,"SPJ":1275.88,"ZDF":1.5828}
        ],"count":3915},"success":true,"message":"ok","code":0}"#;
        let l = parse_margin_days(raw).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].date, "2026-06-11"); // 最新在前
        assert!((l[0].rzye - 19190663674.0).abs() < 1.0);
        assert!((l[0].rqye - 170919165.0).abs() < 1.0);
        assert!((l[0].rzrqye - 19361582839.0).abs() < 1.0);
        assert!((l[0].rzjme - (-68727865.0)).abs() < 1.0);
        assert!((l[1].rzrqye - 19438569746.8).abs() < 0.01);
        // 余额较上日变动（前端口径）：最新 - 上日 < 0（融资余额下降）
        assert!(l[0].rzye - l[1].rzye < 0.0);
        // 非两融标的：9201 → Some(空)（实测形状），与无效响应（None）区分
        assert_eq!(parse_margin_days(r#"{"version":null,"result":null,"success":false,"message":"返回数据为空","code":9201}"#).unwrap().len(), 0);
        assert!(parse_margin_days(r#"{"success":false,"code":9501,"message":"报表配置不存在"}"#).is_none());
        assert!(parse_margin_days("x").is_none());
    }

    #[test]
    fn test_parse_lhb_detail() {
        // 2026-06-12 实测真实形状（永鼎股份 2026-06-04 上榜，截取关键字段）
        let raw = r#"{"result":{"pages":24,"data":[
            {"TRADE_DATE":"2026-06-04 00:00:00","SECURITY_CODE":"600105",
             "EXPLANATION":"非ST、*ST和S证券连续三个交易日内收盘价格涨幅偏离值累计达到20%的证券",
             "BILLBOARD_BUY_AMT":4240434963,"BILLBOARD_SELL_AMT":3173407855.4,
             "BILLBOARD_NET_AMT":1067027107.6,"CLOSE_PRICE":54.29,"CHANGE_RATE":4.5647},
            {"TRADE_DATE":"2026-04-10 00:00:00","SECURITY_CODE":"600105",
             "EXPLANATION":"非ST、*ST和S证券连续三个交易日内收盘价格涨幅偏离值累计达到20%的证券",
             "BILLBOARD_BUY_AMT":3130276977.62,"BILLBOARD_SELL_AMT":2473356675.91,
             "BILLBOARD_NET_AMT":656920301.71,"CLOSE_PRICE":38.1,"CHANGE_RATE":9.9885}
        ],"count":72},"success":true,"message":"ok","code":0}"#;
        let l = parse_lhb_detail(raw).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].date, "2026-06-04"); // 最新在前
        assert!((l[0].net - 1067027107.6).abs() < 0.01);
        assert!((l[0].buy - 4240434963.0).abs() < 1.0);
        assert!((l[0].sell - 3173407855.4).abs() < 0.01);
        assert!(l[0].reason.contains("涨幅偏离值"));
        // 从未上榜：9201"返回数据为空" → Some(空)（诚实区分"没上过榜"与"取数失败"）
        assert_eq!(parse_lhb_detail(r#"{"version":null,"result":null,"success":false,"message":"返回数据为空","code":9201}"#).unwrap().len(), 0);
        assert!(parse_lhb_detail(r#"{"success":false,"code":500}"#).is_none());
        assert!(parse_lhb_detail("x").is_none());
    }

    #[test]
    fn test_split_code() {
        assert_eq!(split_code("sh600519").unwrap(), ("600519".to_string(), "SH"));
        assert_eq!(split_code("SZ000001").unwrap(), ("000001".to_string(), "SZ"));
        assert!(split_code("bj430047").is_err());
        assert!(split_code("600519").is_err());
    }
}

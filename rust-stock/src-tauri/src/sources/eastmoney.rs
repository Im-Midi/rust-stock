// eastmoney.rs — 东方财富数据源
// canonical → secid 转换在这里完成（前端不再关心各源的私有格式）。

use super::QuoteSource;
use crate::quote::Quote;

pub struct EastmoneySource;

/// canonical → 东财 secid（前缀：1.=沪 0.=深 100.=国际指数）
pub fn to_secid(code: &str) -> Option<String> {
    match code {
        "int_dji" => Some("100.DJIA".into()),
        "int_nasdaq" => Some("100.NDX".into()),
        c if c.starts_with("sh") && c.len() == 8 => Some(format!("1.{}", &c[2..])),
        c if c.starts_with("sz") && c.len() == 8 => Some(format!("0.{}", &c[2..])),
        _ => None,
    }
}

#[async_trait::async_trait]
impl QuoteSource for EastmoneySource {
    fn id(&self) -> &'static str {
        "eastmoney"
    }
    fn display_name(&self) -> &'static str {
        "东方财富"
    }

    #[cfg(feature = "net")]
    async fn fetch(&self, codes: &[String]) -> Result<Vec<Quote>, String> {
        let secids: Vec<String> = codes.iter().filter_map(|c| to_secid(c)).collect();
        if secids.is_empty() {
            return Err("没有可识别的代码".into());
        }
        let refs: Vec<&str> = secids.iter().map(|s| s.as_str()).collect();
        crate::quote::fetch_eastmoney(&refs).await
    }

    #[cfg(not(feature = "net"))]
    async fn fetch(&self, _codes: &[String]) -> Result<Vec<Quote>, String> {
        Err("net feature 未启用".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_secid() {
        assert_eq!(to_secid("sh600519").unwrap(), "1.600519");
        assert_eq!(to_secid("sh000001").unwrap(), "1.000001");
        assert_eq!(to_secid("sz000001").unwrap(), "0.000001");
        assert_eq!(to_secid("int_dji").unwrap(), "100.DJIA");
        assert_eq!(to_secid("int_nasdaq").unwrap(), "100.NDX");
        assert!(to_secid("xx123").is_none());
        assert!(to_secid("sh12345").is_none()); // 长度不对
    }
}

// sina.rs — 新浪财经数据源
// canonical 格式与新浪私有格式一致（本项目的统一格式就是按新浪定的），直接透传。

use super::QuoteSource;
use crate::quote::Quote;

pub struct SinaSource;

#[async_trait::async_trait]
impl QuoteSource for SinaSource {
    fn id(&self) -> &'static str {
        "sina"
    }
    fn display_name(&self) -> &'static str {
        "新浪财经"
    }

    #[cfg(feature = "net")]
    async fn fetch(&self, codes: &[String]) -> Result<Vec<Quote>, String> {
        let refs: Vec<&str> = codes.iter().map(|s| s.as_str()).collect();
        crate::quote::fetch_sina(&refs).await
    }

    #[cfg(not(feature = "net"))]
    async fn fetch(&self, _codes: &[String]) -> Result<Vec<Quote>, String> {
        Err("net feature 未启用".into())
    }
}

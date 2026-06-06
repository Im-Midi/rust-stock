// sources/mod.rs — 行情数据源抽象
//
// 新增一个数据源只需两步：
//   1) 新建 xxx.rs 实现 QuoteSource trait
//   2) 在下面 all() 里登记一行
// 其余代码（命令层、前端）零改动 —— 前端的数据源下拉框由 list() 动态生成。
//
// 统一代码格式（canonical）：
//   A股个股/指数用新浪风格：sh600519 / sz000001 / sh000300
//   国际指数用 int_ 前缀：int_dji / int_nasdaq
// 各数据源在自己的实现里把 canonical 转成私有格式（如东财 secid）。

mod eastmoney;
mod sina;

pub use eastmoney::EastmoneySource;
pub use sina::SinaSource;

use crate::quote::Quote;

#[async_trait::async_trait]
pub trait QuoteSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    /// codes 为统一格式，由实现自行转换
    async fn fetch(&self, codes: &[String]) -> Result<Vec<Quote>, String>;
}

static SINA: SinaSource = SinaSource;
static EASTMONEY: EastmoneySource = EastmoneySource;

/// 数据源注册表（新增源在这里加一行）
pub fn all() -> Vec<&'static dyn QuoteSource> {
    vec![&SINA, &EASTMONEY]
}

pub fn get(id: &str) -> Option<&'static dyn QuoteSource> {
    all().into_iter().find(|s| s.id() == id)
}

#[derive(serde::Serialize)]
pub struct SourceInfo {
    pub id: &'static str,
    pub name: &'static str,
}

pub fn list() -> Vec<SourceInfo> {
    all()
        .iter()
        .map(|s| SourceInfo { id: s.id(), name: s.display_name() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry() {
        assert!(get("sina").is_some());
        assert!(get("eastmoney").is_some());
        assert!(get("nope").is_none());
        assert_eq!(list().len(), 2);
        assert_eq!(get("sina").unwrap().display_name(), "新浪财经");
    }
}

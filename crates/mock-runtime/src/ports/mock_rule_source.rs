//! Mock 规则来源端口:运行时按需取一组(已启用的)规则供匹配。
//!
//! 生产实现读 PG(`ms_api_mock` 等);测试用内存实现。匹配本身不碰此端口,保持纯函数。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::MockRule;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("rule source error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait MockRuleSource: Send + Sync {
    /// 取出待匹配的规则集(通常按项目/服务范围过滤,且只含启用项)。
    async fn active_rules(&self) -> Result<Vec<MockRule>, SourceError>;
}

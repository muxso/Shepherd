//! 内存规则来源(测试 / 本地起 Mock 服务)。

use async_trait::async_trait;

use crate::domain::MockRule;
use crate::ports::{MockRuleSource, SourceError};

#[derive(Clone, Default)]
pub struct InMemoryRuleSource {
    rules: Vec<MockRule>,
}

impl InMemoryRuleSource {
    pub fn new(rules: Vec<MockRule>) -> Self {
        Self { rules }
    }
}

#[async_trait]
impl MockRuleSource for InMemoryRuleSource {
    async fn active_rules(&self) -> Result<Vec<MockRule>, SourceError> {
        Ok(self.rules.clone())
    }
}

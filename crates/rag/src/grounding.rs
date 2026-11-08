//! Read-only business evidence shared by web chat and channel adapters.
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use webauth::AuthUser;

use crate::domain::Result;

#[derive(Debug, Clone, Serialize)]
pub struct GroundedAnswer {
    pub content: String,
    pub sources: Vec<Value>,
    pub charts: Vec<Value>,
    pub scope: Value,
}

pub struct QueryContext {
    pub user: AuthUser,
    pub project_id: String,
    pub question: String,
    pub history: Vec<(String, String)>,
    pub at_ms: i64,
}

#[async_trait]
pub trait QuerySource: Send + Sync {
    /// None means ordinary document retrieval; failed business queries must not become guesses.
    async fn answer(
        &self,
        context: &QueryContext,
        authorization: &str,
    ) -> Result<Option<GroundedAnswer>>;
    async fn reference_links(
        &self,
        _context: &QueryContext,
        _authorization: &str,
        sources: Vec<Value>,
    ) -> Result<Vec<Value>> {
        Ok(sources)
    }
}

/// Author/date summaries are candidates, not proof of a dependency edit or a production incident.
pub fn needs_change_proof(question: &str) -> bool {
    let value = question.to_lowercase();
    let actor = ["谁", "哪个人", "who "].iter().any(|term| value.contains(term));
    let change =
        ["升级", "改了", "修改了", "改动了", "合并了", "发布了", "回滚了", "changed", "upgraded"]
            .iter()
            .any(|term| value.contains(term));
    let procedure =
        ["谁负责", "流程", "制度", "规范", "审批", "指南"].iter().any(|term| value.contains(term));
    actor && change && !procedure
}

#[cfg(test)]
mod tests {
    #[test]
    fn historical_summaries_do_not_authorize_change_attribution() {
        assert!(super::needs_change_proof("谁升级了 sample-json 版本，现在生产报错"));
        assert!(super::needs_change_proof("扫描全部 git，分析谁升级了 sample-json"));
        assert!(!super::needs_change_proof("谁负责升级审批流程"));
        assert!(!super::needs_change_proof("本周成员甲有没有提交"));
    }
}

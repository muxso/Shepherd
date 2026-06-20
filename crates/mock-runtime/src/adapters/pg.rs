//! PostgreSQL 规则来源:从 `ms_api_mock` JOIN `ms_api_definition` 读启用的 Mock 规则。
//!
//! 匹配维度:接口定义的 method + path,叠加 mock 的 `match_rule` jsonb(header/query/body/优先级)。
//! 响应取自 mock(status + body)。直读 api-definition 上下文的邻域表(与 api-test 直读
//! ms_environment 同构:跨上下文只读、不引 crate 依赖)。
//! OpenAPI 风格路径参数 `{id}` 翻成匹配引擎的单段通配 `*`,使 `/users/{id}` 命中 `/users/42`。

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{ExtraConditions, MatchRule, MockResponse, MockRule};
use crate::ports::{MockRuleSource, SourceError};

#[derive(Clone)]
pub struct PgMockRuleSource {
    pool: PgPool,
}

impl PgMockRuleSource {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// `/users/{id}/orders` → `/users/*/orders`(OpenAPI 路径参数 → 单段通配)。
fn path_to_glob(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            if seg.starts_with('{') && seg.ends_with('}') && seg.len() >= 2 {
                "*"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[async_trait]
impl MockRuleSource for PgMockRuleSource {
    async fn active_rules(&self) -> Result<Vec<MockRule>, SourceError> {
        let rows = sqlx::query(
            "SELECT m.id, d.method, d.path, m.match_rule, m.response_status, m.response_body \
             FROM ms_api_mock m JOIN ms_api_definition d ON d.id = m.api_definition_id \
             WHERE m.enabled AND NOT m.deleted AND NOT d.deleted AND d.protocol = 'HTTP'",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SourceError::Backend(e.to_string()))?;

        let mut rules = Vec::with_capacity(rows.len());
        for r in rows {
            let map = |e: sqlx::Error| SourceError::Backend(e.to_string());
            let id: String = r.try_get("id").map_err(map)?;
            let method: String = r.try_get("method").map_err(map)?;
            let path: String = r.try_get("path").map_err(map)?;
            let match_rule: serde_json::Value = r.try_get("match_rule").map_err(map)?;
            let status: i32 = r.try_get("response_status").map_err(map)?;
            let body: Option<String> = r.try_get("response_body").map_err(map)?;
            // match_rule jsonb → 额外条件;形态不符(如默认 {})则宽容回落空条件。
            let extra: ExtraConditions = serde_json::from_value(match_rule).unwrap_or_default();
            rules.push(MockRule {
                id,
                rule: MatchRule::from_definition(&method, &path_to_glob(&path), extra),
                response: MockResponse { status: status as u16, headers: vec![], body },
            });
        }
        Ok(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_params_become_wildcards() {
        assert_eq!(path_to_glob("/users/{id}/orders"), "/users/*/orders");
        assert_eq!(path_to_glob("/ping"), "/ping");
        assert_eq!(path_to_glob("/a/{x}/b/{y}"), "/a/*/b/*");
    }
}

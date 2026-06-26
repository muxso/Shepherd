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

fn parse_response_headers(v: &serde_json::Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let k = item.get("key")?.as_str()?.trim();
                    if k.is_empty() {
                        return None;
                    }
                    let val = item.get("value").and_then(|x| x.as_str()).unwrap_or("");
                    Some((k.to_string(), val.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
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
            "SELECT m.id, d.method, d.path, m.match_rule, m.response_status, m.response_body, \
                    m.response_headers, m.response_delay_ms \
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
            let resp_headers: serde_json::Value =
                r.try_get("response_headers").unwrap_or_else(|_| serde_json::json!([]));
            let headers = parse_response_headers(&resp_headers);
            let delay_ms: i32 = r.try_get("response_delay_ms").unwrap_or(0);
            // 形态不符(如默认 {})宽容回落空条件,不报错。
            let extra: ExtraConditions = serde_json::from_value(match_rule).unwrap_or_default();
            rules.push(MockRule {
                id,
                rule: MatchRule::from_definition(&method, &path_to_glob(&path), extra),
                response: MockResponse {
                    status: status as u16,
                    headers,
                    body,
                    delay_ms: delay_ms.max(0) as u64,
                },
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

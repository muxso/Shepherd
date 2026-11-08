//! System data tools execute bounded reads through the existing authenticated business routes.
mod activity;
mod audit;
mod business;
mod devops;
use async_trait::async_trait;
use axum::{body::Body, http::Request, Router};
use rag::{
    domain::{RagError, Result},
    grounding::{GroundedAnswer, QueryContext, QuerySource},
};
use serde_json::Value;
use tower::ServiceExt;

pub struct SystemQueries {
    routes: Router,
    business: Option<business::Business>,
}
impl SystemQueries {
    pub fn new(routes: Router) -> Self {
        Self { routes, business: None }
    }
    pub fn with_business(
        mut self,
        pool: sqlx::PgPool,
        planner: std::sync::Arc<dyn rag::ports::Chat>,
    ) -> Self {
        self.business = Some(business::Business::new(pool, planner));
        self
    }
    async fn get(&self, path: &str, authorization: &str) -> Result<Value> {
        let request = Request::builder()
            .uri(path)
            .header("Authorization", authorization)
            .body(Body::empty())
            .map_err(|_| RagError::Backend("invalid data query".into()))?;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            self.routes.clone().oneshot(request),
        )
        .await
        .map_err(|_| {
            RagError::Backend("数据查询超时，未得到可核验结果，请缩小范围后重试。".into())
        })?
        .map_err(|_| RagError::Backend("data query unavailable".into()))?;
        if !response.status().is_success() {
            return Err(RagError::Backend(
                match response.status().as_u16() {
                    401 | 403 => "当前账号没有查询这些业务数据的权限。",
                    404 => "该数据能力尚未启用或来源记录不存在。",
                    _ => "数据查询失败，不能将其解释为没有记录。",
                }
                .into(),
            ));
        }
        let body = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .map_err(|_| RagError::Backend("查询结果过大，请缩小时间或人员范围。".into()))?;
        serde_json::from_slice(&body)
            .map_err(|_| RagError::Backend("data query returned an invalid result".into()))
    }
    async fn source(&self, context: &QueryContext, authorization: &str) -> Result<Option<Value>> {
        let value = self.get("/api/devops/sources", authorization).await?;
        let sources: Vec<_> = value["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|source| source["project_id"].as_str() == Some(&context.project_id))
            .cloned()
            .collect();
        if sources.len() > 1 {
            return Err(RagError::Backend(
                "当前项目关联多个云效数据源，请明确选择要查询的数据源。".into(),
            ));
        }
        Ok(sources.into_iter().next())
    }
}

#[async_trait]
impl QuerySource for SystemQueries {
    async fn answer(
        &self,
        context: &QueryContext,
        authorization: &str,
    ) -> Result<Option<GroundedAnswer>> {
        let followup;
        let context = if ["再按", "改成", "换成", "再看", "那按"]
            .iter()
            .any(|term| context.question.starts_with(term))
        {
            if let Some((_, previous)) =
                context.history.iter().rev().find(|(role, _)| role == "user")
            {
                followup = QueryContext {
                    user: context.user.clone(),
                    project_id: context.project_id.clone(),
                    question: format!("{}；{}", previous, context.question),
                    history: Vec::new(),
                    at_ms: context.at_ms,
                };
                &followup
            } else {
                context
            }
        } else {
            context
        };
        if let Some(answer) = devops::answer(self, context, authorization).await? {
            return Ok(Some(answer));
        }
        let intent = activity::Intent::parse(&context.question)
            .or_else(|| audit::requested(context).then(activity::Intent::audit));
        if let Some(intent) = intent {
            if intent.role == "product" && !context.user.can("REQUIREMENT", "READ") {
                return Err(RagError::Backend(
                    "当前账号没有读取需求产出的权限，不能将未授权数据当作零条记录。".into(),
                ));
            }
            if (intent.metric == "bugs" || intent.event_kind == "bugReported")
                && !context.user.can("BUG", "READ")
            {
                return Err(RagError::Backend("当前账号没有读取缺陷数据的权限。".into()));
            }
            if intent.role == "test"
                && !context.user.can("FUNCTIONAL_CASE", "READ")
                && !context.user.can("BUG", "READ")
            {
                return Err(RagError::Backend("当前账号没有读取测试活动的权限。".into()));
            }
            let Some(source) = self.source(context, authorization).await? else {
                return Err(RagError::Backend(
                    "当前项目尚未关联研发统计数据源，不能从知识片段推断统计结果。".into(),
                ));
            };
            let window = activity::window(&context.question, context.at_ms)?;
            let id = source["id"]
                .as_str()
                .ok_or_else(|| RagError::Backend("invalid source identity".into()))?;
            let path = format!(
                "/api/devops/sources/{}/team-activity?role={}&days={}",
                rag::references::component(id),
                intent.role,
                window.days
            );
            let data = self.get(&path, authorization).await?;
            return activity::answer(context, &intent, &window, &source, &data).map(Some);
        }
        if let Some(business) = &self.business {
            return business.answer(context).await;
        }
        Ok(None)
    }
    async fn reference_links(
        &self,
        context: &QueryContext,
        authorization: &str,
        mut references: Vec<Value>,
    ) -> Result<Vec<Value>> {
        let shas: std::collections::BTreeSet<String> = references
            .iter()
            .flat_map(|source| source["commit_shas"].as_array().into_iter().flatten())
            .filter_map(|sha| sha.as_str().map(str::to_owned))
            .take(20)
            .collect();
        if shas.is_empty() {
            return Ok(references);
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        let source =
            match tokio::time::timeout_at(deadline, self.source(context, authorization)).await {
                Ok(Ok(Some(source))) => source,
                _ => return Ok(references),
            };
        let Some(id) = source["id"].as_str() else { return Ok(references) };
        let mut pending = shas.into_iter();
        let mut tasks = tokio::task::JoinSet::new();
        let mut links: std::collections::BTreeMap<String, Vec<Value>> =
            std::collections::BTreeMap::new();
        loop {
            while tasks.len() < 4 {
                let Some(sha) = pending.next() else {
                    break;
                };
                let reader = Self::new(self.routes.clone());
                let token = authorization.to_owned();
                let path=format!("/api/devops/sources/{}/records?kind=commit&search={}&days=365&page=1&pageSize=4",rag::references::component(id),rag::references::component(&sha));
                tasks.spawn(async move { (sha, reader.get(&path, &token).await) });
            }
            if tasks.is_empty() {
                break;
            }
            let result = match tokio::time::timeout_at(deadline, tasks.join_next()).await {
                Ok(result) => result,
                Err(_) => {
                    tasks.abort_all();
                    break;
                }
            };
            let Some(Ok((sha, Ok(result)))) = result else {
                continue;
            };
            for row in result["items"].as_array().into_iter().flatten() {
                let commit = &row["data"];
                if commit["sha"].as_str() != Some(&sha) {
                    continue;
                }
                let url = ["url", "webUrl", "web_url", "sourceUrl", "commitUrl"]
                    .iter()
                    .find_map(|key| commit[key].as_str().and_then(rag::references::safe_url));
                if let Some(url) = url {
                    links.entry(sha.clone()).or_default().push(serde_json::json!({"label":format!("{} · {} · {}",commit["authoredAt"].as_str().unwrap_or_default().chars().take(10).collect::<String>(),commit["repo"].as_str().unwrap_or_default(),sha.chars().take(12).collect::<String>()),"url":url,"repository_id":commit["repoId"].as_str().map(str::to_owned).or_else(||commit["repoId"].as_u64().map(|id|id.to_string()))}));
                }
            }
        }
        for source in &mut references {
            let repo = source["repository_id"].as_str();
            let mut found = Vec::new();
            for sha in
                source["commit_shas"].as_array().into_iter().flatten().filter_map(Value::as_str)
            {
                for entry in links.get(sha).into_iter().flatten() {
                    if repo.is_some() && repo != entry["repository_id"].as_str() {
                        continue;
                    }
                    if !found.iter().any(|item: &Value| item["url"] == entry["url"]) {
                        found.push(entry.clone());
                    }
                }
            }
            if found.len() == 1 {
                source["url"] = found[0]["url"].clone();
            }
            if !found.is_empty() {
                if let Some(existing) = source.get_mut("source_links").and_then(Value::as_array_mut)
                {
                    for item in found {
                        if !existing.iter().any(|entry| entry["url"] == item["url"]) {
                            existing.push(item);
                        }
                    }
                } else {
                    source["source_links"] = serde_json::json!(found);
                }
            }
        }
        Ok(references)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, routing::get, Json};
    use std::sync::Arc;
    use webauth::{AuthUser, PermissionSet, SessionStore};
    type Sessions = Arc<dyn SessionStore>;
    async fn sources(user: AuthUser) -> std::result::Result<Json<Value>, axum::http::StatusCode> {
        if !user.can("PROJECT", "READ") {
            return Err(axum::http::StatusCode::FORBIDDEN);
        }
        Ok(Json(serde_json::json!({"items":[{"id":"source","project_id":"project"}]})))
    }
    async fn activity(_user: AuthUser, State(_sessions): State<Sessions>) -> Json<Value> {
        Json(
            serde_json::json!({"from":"2026-11-02","through":"2026-11-08","members":[{"name":"成员甲","group":"研发组"}],"events":[{"kind":"commit","actor":"成员甲","group":"研发组","date":"2026-11-04","at":"2026-11-04T18:00:00+08:00"}],"coverage":{"complete":true}}),
        )
    }
    async fn setup(permission: &str) -> (SystemQueries, String, QueryContext) {
        let sessions: Sessions = Arc::new(webauth::testing::InMemorySessionStore::new());
        let permissions = PermissionSet::from_raw([permission]).expect("permissions");
        let token = sessions.create("user", permissions.clone(), 3600).await.expect("session");
        let routes = Router::new()
            .route("/api/devops/sources", get(sources))
            .route("/api/devops/sources/source/team-activity", get(activity))
            .with_state(sessions);
        let context = QueryContext {
            user: AuthUser { user_id: "user".into(), permissions },
            project_id: "project".into(),
            question: "云效数据按组发我，每个组的代码热力图".into(),
            history: Vec::new(),
            at_ms: 1794146400000,
        };
        (SystemQueries::new(routes), format!("Bearer {token}"), context)
    }
    #[tokio::test]
    async fn statistics_reuse_authenticated_routes_and_do_not_call_document_search() {
        let (provider, token, context) = setup("PROJECT:READ").await;
        let answer = provider.answer(&context, &token).await.expect("query").expect("statistics");
        assert_eq!(answer.scope["returned"], 1);
        assert_eq!(answer.charts[0]["kind"], "activity_heatmap");
    }
    #[tokio::test]
    async fn denied_queries_remain_errors_instead_of_becoming_zero_or_semantic_guesses() {
        let (provider, token, context) = setup("RAG:READ").await;
        assert!(provider.answer(&context, &token).await.is_err());
    }
    #[tokio::test]
    async fn sources_from_other_projects_are_not_used() {
        let (provider, token, mut context) = setup("PROJECT:READ").await;
        context.project_id = "other-project".into();
        assert!(provider.answer(&context, &token).await.is_err());
    }
}

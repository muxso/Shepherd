use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use task::adapters::HeuristicPlanner;
use task::ports::{PlanError, PlannedTask, Planner, RequirementSpec};

struct HttpPlanner {
    client: reqwest::Client,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlannedTaskDto {
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    acceptance_criteria: Vec<String>,
    #[serde(default)]
    dependencies: Vec<usize>,
}

#[async_trait]
impl Planner for HttpPlanner {
    async fn plan(&self, spec: &RequirementSpec) -> Result<Vec<PlannedTask>, PlanError> {
        let body = json!({
            "requirementId": spec.requirement_id,
            "requirementVersion": spec.requirement_version,
            "title": spec.title,
            "description": spec.description,
            "acceptanceCriteria": spec.acceptance_criteria
        });
        let resp = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| PlanError::Backend(format!("planner 不可达: {e}")))?;
        if !resp.status().is_success() {
            return Err(PlanError::Backend(format!("planner HTTP {}", resp.status())));
        }
        let dtos: Vec<PlannedTaskDto> =
            resp.json().await.map_err(|e| PlanError::Backend(format!("planner 响应解析失败: {e}")))?;
        Ok(dtos
            .into_iter()
            .map(|d| PlannedTask {
                title: d.title,
                description: d.description,
                acceptance_criteria: d.acceptance_criteria,
                dependencies: d.dependencies,
            })
            .collect())
    }
}

pub fn build_planner() -> Arc<dyn Planner> {
    if let Some(p) = crate::llm::planner() {
        return p;
    }
    match std::env::var("SHEPHERD_PLANNER_URL") {
        Ok(url) if !url.trim().is_empty() => {
            let client = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());
            Arc::new(HttpPlanner { client, url })
        }
        _ => Arc::new(HeuristicPlanner),
    }
}

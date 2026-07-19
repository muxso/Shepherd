use std::sync::Arc;

use crate::domain::{CaseStatus, Plan, PlanMeta, ROOT_GROUP};
use crate::ports::{PlanRepository, RepoError};

use thiserror::Error;

/// Planning docs are stored verbatim; cap the payload so a runaway client
/// cannot bloat the row (~256KB serialized).
pub const PLANNING_MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanAdminError {
    #[error("plan not found")]
    NotFound,
    #[error("plan name must not be empty")]
    EmptyName,
    #[error("invalid payload: {0}")]
    Invalid(String),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

/// Partial update from PUT /test-plan/{id}; None = keep the current value.
/// Sentinels for clearable fields: module_id Some("") clears; start/end <= 0 clears;
/// group_id ""/"NONE" moves the plan back to the root.
#[derive(Debug, Clone, Default)]
pub struct PlanPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub module_id: Option<String>,
    pub group_id: Option<String>,
    pub start_at_ms: Option<i64>,
    pub end_at_ms: Option<i64>,
    pub allow_duplicate_cases: Option<bool>,
    pub auto_update_status: Option<bool>,
    /// Percent 0..=100 (stored as a 0..=1 fraction).
    pub pass_threshold: Option<f64>,
    /// true archives the plan (hidden from the list), false restores it.
    pub archived: Option<bool>,
}

pub type PlanDetail = (Plan, PlanMeta, Option<serde_json::Value>);

#[derive(Clone)]
pub struct PlanAdminUseCase {
    repo: Arc<dyn PlanRepository>,
}

impl PlanAdminUseCase {
    pub fn new(repo: Arc<dyn PlanRepository>) -> Self {
        Self { repo }
    }

    pub async fn detail(&self, id: &str) -> Result<PlanDetail, PlanAdminError> {
        self.repo.detail(id).await?.ok_or(PlanAdminError::NotFound)
    }

    /// Non-archived plans of a project (groups included), newest first.
    pub async fn list(&self, project_id: &str) -> Result<Vec<(Plan, PlanMeta)>, PlanAdminError> {
        Ok(self.repo.list(project_id).await?)
    }

    /// Merge the patch over the current meta, then write the full row back.
    pub async fn update(&self, id: &str, patch: PlanPatch) -> Result<PlanDetail, PlanAdminError> {
        let (plan, mut meta, _) = self.repo.detail(id).await?.ok_or(PlanAdminError::NotFound)?;
        let name = match &patch.name {
            Some(n) => {
                let n = n.trim();
                if n.is_empty() {
                    return Err(PlanAdminError::EmptyName);
                }
                n.to_string()
            }
            None => plan.name.clone(),
        };
        if let Some(d) = patch.description {
            meta.description = d;
        }
        if let Some(tags) = patch.tags {
            meta.tags =
                tags.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        }
        if let Some(m) = patch.module_id {
            meta.module_id = if m.trim().is_empty() { None } else { Some(m.trim().to_string()) };
        }
        if let Some(g) = patch.group_id {
            let g = g.trim();
            meta.group_id = if g.is_empty() { ROOT_GROUP.to_string() } else { g.to_string() };
        }
        if let Some(ms) = patch.start_at_ms {
            meta.start_at_ms = (ms > 0).then_some(ms);
        }
        if let Some(ms) = patch.end_at_ms {
            meta.end_at_ms = (ms > 0).then_some(ms);
        }
        if let Some(v) = patch.allow_duplicate_cases {
            meta.allow_duplicate_cases = v;
        }
        if let Some(v) = patch.auto_update_status {
            meta.auto_update_status = v;
        }
        if let Some(p) = patch.pass_threshold {
            if !(0.0..=100.0).contains(&p) {
                return Err(PlanAdminError::Invalid("passThreshold must be 0..=100".into()));
            }
            meta.pass_threshold = p / 100.0;
        }
        if !self.repo.update_meta(id, &name, &meta).await? {
            return Err(PlanAdminError::NotFound);
        }
        if let Some(archived) = patch.archived {
            if !self.repo.set_archived(id, archived).await? {
                return Err(PlanAdminError::NotFound);
            }
        }
        self.repo.detail(id).await?.ok_or(PlanAdminError::NotFound)
    }

    /// Store the doc verbatim, then fully sync node-linked cases/scenarios into the
    /// flat plan-case links. Names resolve from the optional root-level
    /// caseNames/scenarioNames maps, falling back to the id.
    ///
    /// Sync rule: after linking the union of the doc's caseIds/scenarioIds, flat
    /// links absent from that union are removed — but only when the doc still has
    /// nodes (an empty doc means planning is not in use and never prunes) and only
    /// PENDING links without a recorded result. Executed rows are history and
    /// survive. Links added manually via the cases tab follow the same
    /// no-result rule: an un-executed manual link not present in a non-empty
    /// planning doc is pruned too.
    pub async fn save_planning(
        &self,
        id: &str,
        doc: serde_json::Value,
    ) -> Result<usize, PlanAdminError> {
        if !doc.is_object() {
            return Err(PlanAdminError::Invalid("planning doc must be a JSON object".into()));
        }
        let size = serde_json::to_string(&doc).map(|s| s.len()).unwrap_or(usize::MAX);
        if size > PLANNING_MAX_BYTES {
            return Err(PlanAdminError::Invalid(format!(
                "planning doc too large: {size} bytes (max {PLANNING_MAX_BYTES})"
            )));
        }
        if !self.repo.set_planning(id, &doc).await? {
            return Err(PlanAdminError::NotFound);
        }
        let union = collect_linked(&doc);
        let mut linked = 0usize;
        for (case_id, name) in &union {
            self.repo.link_case(id, case_id, name).await?;
            linked += 1;
        }
        let has_nodes = doc.get("nodes").and_then(|n| n.as_array()).is_some_and(|a| !a.is_empty());
        if has_nodes {
            let in_union: std::collections::BTreeSet<&str> =
                union.iter().map(|(id, _)| id.as_str()).collect();
            for c in self.repo.list_cases(id).await? {
                let untouched = c.status == CaseStatus::Pending && c.result.is_none();
                if untouched && !in_union.contains(c.case_id.as_str()) {
                    self.repo.unlink_case(id, &c.case_id).await?;
                }
            }
        }
        Ok(linked)
    }
}

/// Union of all nodes' caseIds + scenarioIds, deduplicated, with display names.
fn collect_linked(doc: &serde_json::Value) -> Vec<(String, String)> {
    let name_of = |map: &str, id: &str| -> String {
        doc.get(map).and_then(|m| m.get(id)).and_then(|v| v.as_str()).unwrap_or(id).to_string()
    };
    let mut seen = std::collections::BTreeMap::new();
    let mut stack: Vec<&serde_json::Value> =
        doc.get("nodes").and_then(|n| n.as_array()).map(|a| a.iter().collect()).unwrap_or_default();
    while let Some(node) = stack.pop() {
        for (key, map) in [("caseIds", "caseNames"), ("scenarioIds", "scenarioNames")] {
            if let Some(ids) = node.get(key).and_then(|v| v.as_array()) {
                for id in ids.iter().filter_map(|v| v.as_str()) {
                    seen.entry(id.to_string()).or_insert_with(|| name_of(map, id));
                }
            }
        }
        if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
            stack.extend(children.iter());
        }
    }
    seen.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryPlanRepository;
    use crate::domain::{NewPlan, PlanType};
    use serde_json::json;

    async fn seeded() -> (Arc<InMemoryPlanRepository>, Plan) {
        let repo = Arc::new(InMemoryPlanRepository::new());
        let p = repo.seed(NewPlan::new("p1", "冒烟", PlanType::Plan, ROOT_GROUP).expect("v")).await;
        (repo, p)
    }

    #[tokio::test]
    async fn detail_of_missing_plan_not_found() {
        let uc = PlanAdminUseCase::new(Arc::new(InMemoryPlanRepository::new()));
        assert_eq!(uc.detail("ghost").await.unwrap_err(), PlanAdminError::NotFound);
    }

    #[tokio::test]
    async fn update_merges_and_roundtrips() {
        let (repo, p) = seeded().await;
        let uc = PlanAdminUseCase::new(repo);
        let patch = PlanPatch {
            name: Some("回归".into()),
            description: Some("回归说明".into()),
            tags: Some(vec!["核心".into(), " ".into()]),
            module_id: Some("m1".into()),
            start_at_ms: Some(1000),
            end_at_ms: Some(2000),
            allow_duplicate_cases: Some(false),
            pass_threshold: Some(80.0),
            ..Default::default()
        };
        let (plan, meta, _) = uc.update(&p.id, patch).await.expect("update");
        assert_eq!(plan.name, "回归");
        assert_eq!(meta.description, "回归说明");
        assert_eq!(meta.tags, vec!["核心".to_string()]);
        assert_eq!(meta.module_id.as_deref(), Some("m1"));
        assert_eq!(meta.start_at_ms, Some(1000));
        assert!(!meta.allow_duplicate_cases);
        assert!((meta.pass_threshold - 0.8).abs() < 1e-9);
        // Partial update keeps the rest.
        let (_, meta2, _) = uc
            .update(&p.id, PlanPatch { module_id: Some(String::new()), ..Default::default() })
            .await
            .expect("clear module");
        assert_eq!(meta2.module_id, None);
        assert_eq!(meta2.description, "回归说明");
    }

    #[tokio::test]
    async fn update_rejects_blank_name_and_bad_threshold() {
        let (repo, p) = seeded().await;
        let uc = PlanAdminUseCase::new(repo);
        assert_eq!(
            uc.update(&p.id, PlanPatch { name: Some("  ".into()), ..Default::default() })
                .await
                .unwrap_err(),
            PlanAdminError::EmptyName
        );
        assert!(matches!(
            uc.update(&p.id, PlanPatch { pass_threshold: Some(120.0), ..Default::default() })
                .await
                .unwrap_err(),
            PlanAdminError::Invalid(_)
        ));
    }

    #[tokio::test]
    async fn save_planning_stores_doc_and_links_union() {
        let (repo, p) = seeded().await;
        let uc = PlanAdminUseCase::new(repo.clone());
        let doc = json!({
            "nodes": [{
                "id": "n1", "name": "功能用例", "kind": "category",
                "caseIds": ["c1"],
                "children": [{
                    "id": "n2", "name": "登录", "kind": "point",
                    "caseIds": ["c1", "c2"], "scenarioIds": ["s1"],
                    "config": {"inherit": false, "mode": "serial"}
                }]
            }],
            "caseNames": {"c1": "健康检查", "c2": "登录接口"},
            "scenarioNames": {"s1": "登录链路"}
        });
        let linked = uc.save_planning(&p.id, doc.clone()).await.expect("save");
        assert_eq!(linked, 3);
        let (_, _, stored) = uc.detail(&p.id).await.expect("detail");
        assert_eq!(stored, Some(doc));
        let cases = repo.list_cases(&p.id).await.expect("cases");
        assert_eq!(cases.len(), 3);
        assert!(cases.iter().any(|c| c.case_id == "c1" && c.name == "健康检查"));
        assert!(cases.iter().any(|c| c.case_id == "s1" && c.name == "登录链路"));
    }

    #[tokio::test]
    async fn save_planning_prunes_unexecuted_links_dropped_from_doc() {
        let (repo, p) = seeded().await;
        let uc = PlanAdminUseCase::new(repo.clone());
        let doc2 = json!({"nodes": [{"id": "n1", "caseIds": ["c1", "c2"]}]});
        uc.save_planning(&p.id, doc2).await.expect("save two");
        assert_eq!(repo.list_cases(&p.id).await.expect("cases").len(), 2);

        // Drop c2 from the doc: its PENDING/no-result link goes away.
        let doc1 = json!({"nodes": [{"id": "n1", "caseIds": ["c1"]}]});
        uc.save_planning(&p.id, doc1).await.expect("save one");
        let cases = repo.list_cases(&p.id).await.expect("cases");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].case_id, "c1");
    }

    #[tokio::test]
    async fn save_planning_keeps_executed_links_dropped_from_doc() {
        let (repo, p) = seeded().await;
        let uc = PlanAdminUseCase::new(repo.clone());
        let doc2 = json!({"nodes": [{"id": "n1", "caseIds": ["c1", "c2"]}]});
        uc.save_planning(&p.id, doc2).await.expect("save two");
        repo.record_result(&p.id, "c2", crate::domain::CaseStatus::Success, None)
            .await
            .expect("exec c2");

        let doc1 = json!({"nodes": [{"id": "n1", "caseIds": ["c1"]}]});
        uc.save_planning(&p.id, doc1).await.expect("save one");
        let cases = repo.list_cases(&p.id).await.expect("cases");
        // The executed c2 row is history and survives the prune.
        assert_eq!(cases.len(), 2);
        assert!(cases.iter().any(|c| c.case_id == "c2"));
    }

    #[tokio::test]
    async fn save_planning_empty_doc_never_prunes() {
        let (repo, p) = seeded().await;
        let uc = PlanAdminUseCase::new(repo.clone());
        // A manual link (cases tab) with no planning doc coverage.
        repo.link_case(&p.id, "m1", "手动用例").await.expect("manual link");
        uc.save_planning(&p.id, json!({"nodes": []})).await.expect("save empty");
        assert_eq!(repo.list_cases(&p.id).await.expect("cases").len(), 1);
    }

    #[tokio::test]
    async fn save_planning_rejects_non_object_and_oversize() {
        let (repo, p) = seeded().await;
        let uc = PlanAdminUseCase::new(repo);
        assert!(matches!(
            uc.save_planning(&p.id, json!([1, 2])).await.unwrap_err(),
            PlanAdminError::Invalid(_)
        ));
        let big = json!({"nodes": [], "blob": "x".repeat(PLANNING_MAX_BYTES)});
        assert!(matches!(
            uc.save_planning(&p.id, big).await.unwrap_err(),
            PlanAdminError::Invalid(_)
        ));
    }
}

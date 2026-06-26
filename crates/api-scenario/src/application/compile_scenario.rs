use std::collections::HashSet;
use std::sync::Arc;

use crate::domain::{
    flatten_step, parse_control, PlanStep, RunnableStep, ScenarioError, StepKind,
};
use crate::ports::{ApiScenarioRepository, RepoError};

use thiserror::Error;

/// 递归深度上限,防止病态嵌套耗尽栈。
const MAX_DEPTH: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompileError {
    #[error("scenario not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Cycle(ScenarioError),
    #[error(transparent)]
    Depth(ScenarioError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CompileScenarioUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl CompileScenarioUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        scenario_id: &str,
    ) -> Result<Vec<RunnableStep>, CompileError> {
        let mut visited = HashSet::new();
        self.compile_inner(scenario_id, &mut visited, 0).await
    }

    // Box::pin 装箱递归 future,绕开 async 自递归的返回类型大小限制。
    fn compile_inner<'a>(
        &'a self,
        scenario_id: &'a str,
        visited: &'a mut HashSet<String>,
        depth: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<RunnableStep>, CompileError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if depth >= MAX_DEPTH {
                return Err(CompileError::Depth(ScenarioError::MaxDepthExceeded(MAX_DEPTH)));
            }
            if !visited.insert(scenario_id.to_string()) {
                return Err(CompileError::Cycle(ScenarioError::CycleDetected(
                    scenario_id.to_string(),
                )));
            }

            let scenario = self
                .repo
                .get_scenario(scenario_id)
                .await?
                .ok_or_else(|| CompileError::NotFound(scenario_id.to_string()))?;

            let mut out = Vec::new();
            for step in &scenario.steps {
                match &step.kind {
                    StepKind::Request(_) | StepKind::Case { .. } => {
                        if let Some(runnable) = flatten_step(step) {
                            out.push(runnable);
                        }
                    }
                    StepKind::Scenario { scenario_id: sub_id } => {
                        let sub = self.compile_inner(sub_id, visited, depth + 1).await?;
                        out.extend(sub);
                    }
                    StepKind::Control { .. } => {}
                }
            }

            visited.remove(scenario_id);
            Ok(out)
        })
    }

    pub async fn compile_plan(&self, scenario_id: &str) -> Result<Vec<PlanStep>, CompileError> {
        let mut visited = HashSet::new();
        self.compile_plan_inner(scenario_id, &mut visited, 0).await
    }

    fn compile_plan_inner<'a>(
        &'a self,
        scenario_id: &'a str,
        visited: &'a mut HashSet<String>,
        depth: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<PlanStep>, CompileError>> + Send + 'a>,
    > {
        Box::pin(async move {
            if depth >= MAX_DEPTH {
                return Err(CompileError::Depth(ScenarioError::MaxDepthExceeded(MAX_DEPTH)));
            }
            if !visited.insert(scenario_id.to_string()) {
                return Err(CompileError::Cycle(ScenarioError::CycleDetected(
                    scenario_id.to_string(),
                )));
            }
            let scenario = self
                .repo
                .get_scenario(scenario_id)
                .await?
                .ok_or_else(|| CompileError::NotFound(scenario_id.to_string()))?;

            let mut out = Vec::new();
            for step in &scenario.steps {
                match &step.kind {
                    StepKind::Case { case_id } => out.push(PlanStep::Case(case_id.clone())),
                    StepKind::Request(req) => out.push(PlanStep::Request(req.clone())),
                    StepKind::Control { control, payload } => {
                        out.push(parse_control(*control, payload))
                    }
                    StepKind::Scenario { scenario_id: sub_id } => {
                        let sub = self.compile_plan_inner(sub_id, visited, depth + 1).await?;
                        out.extend(sub);
                    }
                }
            }
            visited.remove(scenario_id);
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::application::AddStepUseCase;
    use crate::domain::{InlineRequest, NewApiScenario, NewScenarioStep, RefMode, StepKind};

    async fn add(
        repo: &Arc<InMemoryApiScenarioRepository>,
        scenario_id: &str,
        order: i32,
        kind: StepKind,
    ) {
        let uc = AddStepUseCase::new(repo.clone());
        let step = NewScenarioStep::new(order, kind, RefMode::Reference, None).expect("valid");
        uc.execute(scenario_id, &step).await.expect("add");
    }

    async fn new_scenario(repo: &Arc<InMemoryApiScenarioRepository>, name: &str) -> String {
        repo.insert_scenario(&NewApiScenario::new("p1", name).expect("valid"))
            .await
            .expect("insert")
            .id
    }

    #[tokio::test]
    async fn compiles_flat_scenario_in_order() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let id = new_scenario(&repo, "flat").await;
        let req = InlineRequest::new("GET", "http://a", None).expect("valid");
        add(&repo, &id, 0, StepKind::Request(req)).await;
        add(&repo, &id, 1, StepKind::Case { case_id: "case-1".into() }).await;

        let uc = CompileScenarioUseCase::new(repo);
        let steps = uc.execute(&id).await.expect("compile");
        assert_eq!(steps.len(), 2);
        assert!(steps[0].request.is_some());
        assert_eq!(steps[1].case_id.as_deref(), Some("case-1"));
    }

    #[tokio::test]
    async fn compile_plan_keeps_controller_hierarchy() {
        use crate::domain::{ControlKind, PlanStep};
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let id = new_scenario(&repo, "ctrl").await;
        add(&repo, &id, 0, StepKind::Case { case_id: "c0".into() }).await;
        let payload = serde_json::json!({"times": 3, "children": [{"kind":"CASE","refId":"c1"}]});
        add(&repo, &id, 1, StepKind::Control { control: ControlKind::Loop, payload }).await;

        let plan = CompileScenarioUseCase::new(repo).compile_plan(&id).await.expect("plan");
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0], PlanStep::Case("c0".into()));
        match &plan[1] {
            PlanStep::Loop { times, body } => {
                assert_eq!(*times, 3);
                assert_eq!(body, &vec![PlanStep::Case("c1".into())]);
            }
            other => panic!("expected Loop, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn compiles_nested_subscenario_flattened() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let child = new_scenario(&repo, "child").await;
        add(&repo, &child, 0, StepKind::Case { case_id: "c-child".into() }).await;

        let parent = new_scenario(&repo, "parent").await;
        add(&repo, &parent, 0, StepKind::Case { case_id: "c-pre".into() }).await;
        add(&repo, &parent, 1, StepKind::Scenario { scenario_id: child.clone() }).await;
        add(&repo, &parent, 2, StepKind::Case { case_id: "c-post".into() }).await;

        let uc = CompileScenarioUseCase::new(repo);
        let steps = uc.execute(&parent).await.expect("compile");
        let ids: Vec<_> = steps.iter().filter_map(|s| s.case_id.as_deref()).collect();
        assert_eq!(ids, vec!["c-pre", "c-child", "c-post"]);
    }

    #[tokio::test]
    async fn detects_direct_cycle() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let a = new_scenario(&repo, "a").await;
        add(&repo, &a, 0, StepKind::Scenario { scenario_id: a.clone() }).await;

        let uc = CompileScenarioUseCase::new(repo);
        let err = uc.execute(&a).await.unwrap_err();
        assert!(matches!(err, CompileError::Cycle(_)));
    }

    #[tokio::test]
    async fn detects_indirect_cycle() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let a = new_scenario(&repo, "a").await;
        let b = new_scenario(&repo, "b").await;
        add(&repo, &a, 0, StepKind::Scenario { scenario_id: b.clone() }).await;
        add(&repo, &b, 0, StepKind::Scenario { scenario_id: a.clone() }).await;

        let uc = CompileScenarioUseCase::new(repo);
        assert!(matches!(uc.execute(&a).await.unwrap_err(), CompileError::Cycle(_)));
    }

    #[tokio::test]
    async fn missing_subscenario_is_not_found() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let a = new_scenario(&repo, "a").await;
        add(&repo, &a, 0, StepKind::Scenario { scenario_id: "ghost".into() }).await;

        let uc = CompileScenarioUseCase::new(repo);
        assert_eq!(uc.execute(&a).await.unwrap_err(), CompileError::NotFound("ghost".into()));
    }

    #[tokio::test]
    async fn missing_root_is_not_found() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let uc = CompileScenarioUseCase::new(repo);
        assert_eq!(uc.execute("ghost").await.unwrap_err(), CompileError::NotFound("ghost".into()));
    }

    #[tokio::test]
    async fn diamond_reuse_is_not_a_cycle() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let d = new_scenario(&repo, "d").await;
        add(&repo, &d, 0, StepKind::Case { case_id: "c-d".into() }).await;
        let b = new_scenario(&repo, "b").await;
        add(&repo, &b, 0, StepKind::Scenario { scenario_id: d.clone() }).await;
        let c = new_scenario(&repo, "c").await;
        add(&repo, &c, 0, StepKind::Scenario { scenario_id: d.clone() }).await;
        let a = new_scenario(&repo, "a").await;
        add(&repo, &a, 0, StepKind::Scenario { scenario_id: b.clone() }).await;
        add(&repo, &a, 1, StepKind::Scenario { scenario_id: c.clone() }).await;

        let uc = CompileScenarioUseCase::new(repo);
        let steps = uc.execute(&a).await.expect("compile");
        assert_eq!(steps.len(), 2);
    }
}

//! 用例:复制场景。整份克隆源场景——基本信息(名称/状态/元信息)+ 全部步骤。
//!
//! 纯组合既有仓储方法,不引入新端口:取源 → 建新 → 回填状态/元信息 → 逐步追加。
//! 名称缺省派生 `{源名}_copy`;状态与 meta(描述/标签/等级/模块/参数)整份带过去;
//! 步骤连同 ref_mode 与 COPY 快照原样克隆(`NewScenarioStep` 复校验,保证不变量)。
//! 不复制执行历史与变更历史——副本是一份全新草稿的"内容",不是审计血缘。

use std::sync::Arc;

use crate::domain::{ApiScenario, NewApiScenario, NewScenarioStep, ScenarioError};
use crate::ports::{ApiScenarioRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CopyScenarioError {
    #[error("scenario not found")]
    NotFound,
    #[error(transparent)]
    Validation(#[from] ScenarioError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CopyScenarioUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl CopyScenarioUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    /// 复制 `source_id` 指向的场景。`name` 给定且非空则用作副本名,否则派生 `{源名}_copy`。
    /// `created_by` 记到副本的创建人。返回已加载步骤的副本场景;源不存在 → NotFound。
    pub async fn execute(
        &self,
        source_id: &str,
        name: Option<&str>,
        created_by: Option<&str>,
    ) -> Result<ApiScenario, CopyScenarioError> {
        let Some(source) = self.repo.get_scenario(source_id).await? else {
            return Err(CopyScenarioError::NotFound);
        };

        let new_name = match name.map(str::trim) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => format!("{}_copy", source.name),
        };

        // 建新场景(steps 为空、meta 默认),随后整体回填名称/状态/元信息。
        let created = NewApiScenario::new(&source.project_id, &new_name)?.with_created_by(created_by);
        let fresh = self.repo.insert_scenario(&created).await?;
        self.repo
            .update_scenario(&fresh.id, &new_name, source.status.as_str(), &source.meta)
            .await?;

        // 逐步克隆(源步骤已按 order 升序);ref_mode 与 COPY 快照原样带过去。
        for step in &source.steps {
            let cloned = NewScenarioStep::new(
                step.order,
                step.kind.clone(),
                step.ref_mode,
                step.snapshot.clone(),
            )?;
            self.repo.add_step(&fresh.id, &cloned).await?;
        }

        // 重新读出副本(含步骤),保证返回形态与 GET /scenario/{id} 一致。
        self.repo
            .get_scenario(&fresh.id)
            .await?
            .ok_or(CopyScenarioError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::application::{AddStepUseCase, CreateScenarioUseCase};
    use crate::domain::{InlineRequest, RefMode, StepKind};

    async fn seed(repo: Arc<dyn ApiScenarioRepository>) -> String {
        let create = CreateScenarioUseCase::new(repo.clone());
        let s = create.execute("p1", "下单链路", Some("alice")).await.expect("create");
        // 带 meta(状态/标签等)与两个步骤(CASE + REQUEST)。
        repo.update_scenario(
            &s.id,
            "下单链路",
            "DEBUGGING",
            &serde_json::json!({ "priority": "P1", "tags": ["核心"] }),
        )
        .await
        .expect("meta");
        let add = AddStepUseCase::new(repo.clone());
        let case = NewScenarioStep::new(0, StepKind::Case { case_id: "c1".into() }, RefMode::Reference, None)
            .expect("case step");
        add.execute(&s.id, &case).await.expect("add case");
        let req = InlineRequest::new("POST", "http://x/order", Some("{}".into())).expect("req");
        let request = NewScenarioStep::new(1, StepKind::Request(req), RefMode::Reference, None)
            .expect("req step");
        add.execute(&s.id, &request).await.expect("add req");
        s.id
    }

    #[tokio::test]
    async fn copies_name_status_meta_and_steps() {
        let repo: Arc<dyn ApiScenarioRepository> = Arc::new(InMemoryApiScenarioRepository::new());
        let src = seed(repo.clone()).await;

        let uc = CopyScenarioUseCase::new(repo.clone());
        let copy = uc.execute(&src, None, Some("bob")).await.expect("copy");

        assert_ne!(copy.id, src);
        assert_eq!(copy.name, "下单链路_copy"); // 默认派生名
        assert_eq!(copy.project_id, "p1");
        assert_eq!(copy.status.as_str(), "DEBUGGING"); // 状态带过去
        assert_eq!(copy.meta["priority"], "P1"); // meta 带过去
        assert_eq!(copy.created_by.as_deref(), Some("bob")); // 创建人是复制者
        assert_eq!(copy.steps.len(), 2);
        assert_eq!(copy.steps[0].kind.kind_str(), "CASE");
        assert_eq!(copy.steps[1].kind.kind_str(), "REQUEST");
        // 源场景不受影响,步骤 id 互不相同(深拷贝)。
        let original = repo.get_scenario(&src).await.expect("get").expect("some");
        assert_eq!(original.steps.len(), 2);
        assert_ne!(copy.steps[0].id, original.steps[0].id);
    }

    #[tokio::test]
    async fn honors_explicit_name() {
        let repo: Arc<dyn ApiScenarioRepository> = Arc::new(InMemoryApiScenarioRepository::new());
        let src = seed(repo.clone()).await;
        let uc = CopyScenarioUseCase::new(repo);
        let copy = uc.execute(&src, Some("  回归_下单  "), None).await.expect("copy");
        assert_eq!(copy.name, "回归_下单"); // 去空白后采用
    }

    #[tokio::test]
    async fn missing_source_is_not_found() {
        let repo: Arc<dyn ApiScenarioRepository> = Arc::new(InMemoryApiScenarioRepository::new());
        let uc = CopyScenarioUseCase::new(repo);
        assert_eq!(uc.execute("ghost", None, None).await.unwrap_err(), CopyScenarioError::NotFound);
    }
}

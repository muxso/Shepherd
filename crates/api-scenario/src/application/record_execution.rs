//! 用例:记录一次场景执行(分发运行后由服务端 run bridge 调用)。
//!
//! 纯编排:把状态字符串与计数转交仓储落库,返回持久化后的执行记录。
//! 状态字符串的合法性由仓储/领域 `ExecutionStatus::parse` 把关(未知回落 Pending)。

use std::sync::Arc;

use crate::domain::ScenarioExecution;
use crate::ports::{ApiScenarioRepository, RepoError};

#[derive(Clone)]
pub struct RecordScenarioExecutionUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl RecordScenarioExecutionUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        scenario_id: &str,
        project_id: &str,
        status: &str,
        case_count: i32,
        report_id: Option<&str>,
    ) -> Result<ScenarioExecution, RepoError> {
        self.repo
            .record_execution(scenario_id, project_id, status, case_count, report_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::domain::ExecutionStatus;

    #[tokio::test]
    async fn records_execution_with_status() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let uc = RecordScenarioExecutionUseCase::new(repo);
        let e = uc.execute("scn-1", "p1", "RUNNING", 4, Some("rep-1")).await.expect("rec");
        assert_eq!(e.status, ExecutionStatus::Running);
        assert_eq!(e.case_count, 4);
        assert_eq!(e.report_id.as_deref(), Some("rep-1"));
        assert!(e.id.starts_with("exec-"));
    }
}

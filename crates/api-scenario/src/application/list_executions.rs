//! 用例:分页列出某场景的执行记录(created_at 降序)。
//!
//! 分页参数校验复用 `kernel::page::PageRequest`(非法参数在 HTTP 层即 400)。
//! 这里只编排:先 count 求 total,再按 offset/limit 取一页,组装 `Page`。

use std::sync::Arc;

use kernel::page::{Page, PageRequest};

use crate::domain::ScenarioExecution;
use crate::ports::{ApiScenarioRepository, RepoError};

#[derive(Clone)]
pub struct ListScenarioExecutionsUseCase {
    repo: Arc<dyn ApiScenarioRepository>,
}

impl ListScenarioExecutionsUseCase {
    pub fn new(repo: Arc<dyn ApiScenarioRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        scenario_id: &str,
        page: PageRequest,
    ) -> Result<Page<ScenarioExecution>, RepoError> {
        let total = self.repo.count_executions(scenario_id).await?;
        let items = self
            .repo
            .list_executions(scenario_id, page.offset(), page.page_size())
            .await?;
        Ok(Page::of(page, total, items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiScenarioRepository;
    use crate::application::RecordScenarioExecutionUseCase;

    #[tokio::test]
    async fn paginates_executions_newest_first() {
        let repo = Arc::new(InMemoryApiScenarioRepository::new());
        let rec = RecordScenarioExecutionUseCase::new(repo.clone());
        for i in 0..3 {
            let status = if i == 2 { "SUCCESS" } else { "PENDING" };
            rec.execute("scn-1", "p1", status, i, None).await.expect("rec");
        }
        // 别的场景不该混入。
        rec.execute("scn-2", "p1", "ERROR", 0, None).await.expect("rec");

        let uc = ListScenarioExecutionsUseCase::new(repo);
        let page = uc.execute("scn-1", PageRequest::new(1, 2).expect("valid")).await.expect("page");
        assert_eq!(page.total, 3);
        assert_eq!(page.total_pages(), 2); // ceil(3/2)
        assert_eq!(page.items.len(), 2);
        // 最新(第三条,SUCCESS)在前。
        assert_eq!(page.items[0].case_count, 2);
    }
}

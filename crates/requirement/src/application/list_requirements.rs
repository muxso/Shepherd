//! 用例:分页列出项目内的需求。复用 `kernel` 的 `PageRequest`/`Page`。

use std::sync::Arc;

use kernel::page::{Page, PageRequest};

use crate::domain::Requirement;
use crate::ports::{RepoError, RequirementRepository};

#[derive(Clone)]
pub struct ListRequirementsUseCase {
    repo: Arc<dyn RequirementRepository>,
}

impl ListRequirementsUseCase {
    pub fn new(repo: Arc<dyn RequirementRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        page: PageRequest,
    ) -> Result<Page<Requirement>, RepoError> {
        let total = self.repo.count_active(project_id).await?;
        let items = self.repo.list_active(project_id, page.offset(), page.page_size()).await?;
        Ok(Page::of(page, total, items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryRequirementRepository;
    use crate::application::CreateRequirementUseCase;

    async fn seed(repo: &Arc<InMemoryRequirementRepository>, project: &str, n: usize) {
        let create = CreateRequirementUseCase::new(repo.clone());
        for i in 1..=n {
            create.execute(project, &format!("R{i}"), "d", &[]).await.expect("seed");
        }
    }

    #[tokio::test]
    async fn paginates_and_excludes_other_projects_and_deleted() {
        let repo = Arc::new(InMemoryRequirementRepository::new());
        seed(&repo, "p1", 3).await;
        seed(&repo, "p2", 2).await;
        let doomed =
            CreateRequirementUseCase::new(repo.clone()).execute("p1", "Doomed", "d", &[]).await.expect("ok");
        repo.soft_delete(&doomed.id);

        let uc = ListRequirementsUseCase::new(repo);
        let page = uc.execute("p1", PageRequest::new(1, 50).expect("v")).await.expect("ok");
        assert_eq!(page.total, 3);
        assert!(page.items.iter().all(|r| !r.deleted && r.project_id == "p1"));
    }
}

//! 用例:分页列出组织内的项目。
//!
//! 复用 `kernel` 的 `PageRequest`/`Page`(阶段 0 已用例钉死分页数学),
//! 本用例只负责把校验过的分页参数翻译成 repo 的 offset/limit,并组装结果页。
//! 体现"共享内核被业务模块复用"。

use std::sync::Arc;

use kernel::page::{Page, PageRequest};

use crate::domain::Project;
use crate::ports::{ProjectRepository, RepoError};

#[derive(Clone)]
pub struct ListProjectsUseCase {
    repo: Arc<dyn ProjectRepository>,
}

impl ListProjectsUseCase {
    pub fn new(repo: Arc<dyn ProjectRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        organization_id: &str,
        page: PageRequest,
    ) -> Result<Page<Project>, RepoError> {
        let total = self.repo.count_active(organization_id).await?;
        let items = self
            .repo
            .list_active(organization_id, page.offset(), page.page_size())
            .await?;
        Ok(Page::of(page, total, items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryProjectRepository;
    use crate::application::CreateProjectUseCase;

    async fn seed(repo: &InMemoryProjectRepository, org: &str, n: usize) {
        let create = CreateProjectUseCase::new(Arc::new(repo.clone()));
        for i in 1..=n {
            create.execute(org, &format!("P{i}")).await.expect("seed ok");
        }
    }

    #[tokio::test]
    async fn first_page_returns_slice_and_total() {
        let repo = InMemoryProjectRepository::new();
        seed(&repo, "org1", 25).await;
        let uc = ListProjectsUseCase::new(Arc::new(repo));

        let page = uc
            .execute("org1", PageRequest::new(1, 10).expect("valid"))
            .await
            .expect("ok");
        assert_eq!(page.total, 25);
        assert_eq!(page.items.len(), 10);
        assert_eq!(page.total_pages(), 3); // ceil(25/10)
        assert_eq!(page.items[0].name, "P1");
    }

    #[tokio::test]
    async fn last_page_returns_remainder() {
        let repo = InMemoryProjectRepository::new();
        seed(&repo, "org1", 25).await;
        let uc = ListProjectsUseCase::new(Arc::new(repo));

        let page = uc
            .execute("org1", PageRequest::new(3, 10).expect("valid"))
            .await
            .expect("ok");
        assert_eq!(page.items.len(), 5); // 21..=25
        assert_eq!(page.items[0].name, "P21");
    }

    #[tokio::test]
    async fn list_excludes_soft_deleted_and_other_orgs() {
        let repo = InMemoryProjectRepository::new();
        seed(&repo, "org1", 3).await;
        seed(&repo, "org2", 2).await; // 另一组织,不应计入 org1
        // 软删除 org1 的一个
        let create = CreateProjectUseCase::new(Arc::new(repo.clone()));
        let extra = create.execute("org1", "Doomed").await.expect("ok");
        repo.soft_delete(&extra.id);

        let uc = ListProjectsUseCase::new(Arc::new(repo));
        let page = uc
            .execute("org1", PageRequest::new(1, 50).expect("valid"))
            .await
            .expect("ok");
        assert_eq!(page.total, 3); // 3 个活的,排除其他组织 + 已删除
        assert_eq!(page.items.len(), 3);
        assert!(page.items.iter().all(|p| !p.deleted && p.organization_id == "org1"));
    }
}

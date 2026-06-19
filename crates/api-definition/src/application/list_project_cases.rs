//! 用例:分页列出项目下的全部用例(含独立用例)。count + list 组装成一页。

use std::sync::Arc;

use crate::domain::ApiCase;
use crate::ports::{ApiDefinitionRepository, RepoError};

use kernel::page::{Page, PageRequest};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListProjectCasesError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ListProjectCasesUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl ListProjectCasesUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        page: PageRequest,
    ) -> Result<Page<ApiCase>, ListProjectCasesError> {
        let total = self.repo.count_cases_by_project(project_id).await?;
        let items = self
            .repo
            .list_cases_by_project(project_id, page.offset(), page.page_size())
            .await?;
        Ok(Page::of(page, total, items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::application::CreateApiCaseUseCase;

    #[tokio::test]
    async fn paginates_project_cases() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let create = CreateApiCaseUseCase::new(repo.clone());
        for i in 0..5 {
            create
                .execute("p1", None, &format!("c{i}"), "GET", "/x", None, serde_json::json!([]), serde_json::json!([]))
                .await
                .expect("ok");
        }
        // 另一个项目的用例不应计入
        create
            .execute("p2", None, "other", "GET", "/x", None, serde_json::json!([]), serde_json::json!([]))
            .await
            .expect("ok");

        let uc = ListProjectCasesUseCase::new(repo);
        let req = PageRequest::new(1, 2).expect("page");
        let page = uc.execute("p1", req).await.expect("ok");
        assert_eq!(page.total, 5);
        assert_eq!(page.total_pages(), 3);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].name, "c0");

        let req2 = PageRequest::new(3, 2).expect("page");
        let page2 = uc.execute("p1", req2).await.expect("ok");
        assert_eq!(page2.items.len(), 1);
        assert_eq!(page2.items[0].name, "c4");
    }
}

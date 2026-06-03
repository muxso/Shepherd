//! 用例:列出项目下的接口定义。

use std::sync::Arc;

use crate::domain::ApiDefinition;
use crate::ports::{ApiDefinitionRepository, RepoError};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ListApiDefinitionsError {
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ListApiDefinitionsUseCase {
    repo: Arc<dyn ApiDefinitionRepository>,
}

impl ListApiDefinitionsUseCase {
    pub fn new(repo: Arc<dyn ApiDefinitionRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiDefinition>, ListApiDefinitionsError> {
        Ok(self.repo.list_definitions(project_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryApiDefinitionRepository;
    use crate::application::CreateApiDefinitionUseCase;
    use crate::domain::ApiProtocol;

    #[tokio::test]
    async fn lists_definitions_for_project() {
        let repo = Arc::new(InMemoryApiDefinitionRepository::new());
        let create = CreateApiDefinitionUseCase::new(repo.clone());
        create.execute("p1", "a", ApiProtocol::Http, "GET", "/a").await.expect("ok");
        create.execute("p1", "b", ApiProtocol::Http, "GET", "/b").await.expect("ok");
        create.execute("p2", "c", ApiProtocol::Http, "GET", "/c").await.expect("ok");

        let uc = ListApiDefinitionsUseCase::new(repo);
        let list = uc.execute("p1").await.expect("ok");
        assert_eq!(list.len(), 2);
    }
}

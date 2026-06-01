//! 用例:创建 skill(同项目内名唯一,忽略软删除)。

use std::sync::Arc;

use thiserror::Error;

use crate::domain::{NewSkill, Skill, SkillError};
use crate::ports::{RepoError, SkillRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateSkillError {
    #[error(transparent)]
    Validation(#[from] SkillError),
    #[error("skill name already exists")]
    NameAlreadyExists,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct CreateSkillUseCase {
    repo: Arc<dyn SkillRepository>,
}

impl CreateSkillUseCase {
    pub fn new(repo: Arc<dyn SkillRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        project_id: &str,
        name: &str,
        description: &str,
        instructions: &str,
        includes: &[String],
    ) -> Result<Skill, CreateSkillError> {
        let new = NewSkill::new(project_id, name, description, instructions, includes)?;
        if self.repo.find_active_by_name(&new.project_id, &new.name).await?.is_some() {
            return Err(CreateSkillError::NameAlreadyExists);
        }
        Ok(self.repo.insert(&new).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemorySkillRepository;

    fn uc() -> CreateSkillUseCase {
        CreateSkillUseCase::new(Arc::new(InMemorySkillRepository::new()))
    }

    #[tokio::test]
    async fn creates_and_rejects_duplicate() {
        let uc = uc();
        uc.execute("p1", "Rust", "", "用 thiserror", &[]).await.expect("ok");
        assert_eq!(
            uc.execute("p1", "Rust", "", "x", &[]).await.unwrap_err(),
            CreateSkillError::NameAlreadyExists
        );
        assert!(uc.execute("p2", "Rust", "", "x", &[]).await.is_ok()); // 别的项目可同名
    }

    #[tokio::test]
    async fn propagates_validation() {
        assert!(matches!(
            uc().execute("p1", "  ", "", "x", &[]).await.unwrap_err(),
            CreateSkillError::Validation(SkillError::EmptyName)
        ));
    }
}

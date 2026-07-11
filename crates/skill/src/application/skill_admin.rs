use std::sync::Arc;

use crate::domain::{Composition, Skill, SkillError, SkillLibrary};
use crate::ports::{RepoError, SkillRepository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillCmdError {
    NotFound,
    NameExists,
    Validation(SkillError),
    Compose(SkillError),
    Repo(RepoError),
}

impl From<RepoError> for SkillCmdError {
    fn from(e: RepoError) -> Self {
        Self::Repo(e)
    }
}
impl From<SkillError> for SkillCmdError {
    fn from(e: SkillError) -> Self {
        Self::Validation(e)
    }
}

#[derive(Clone)]
pub struct SkillService {
    repo: Arc<dyn SkillRepository>,
}

impl SkillService {
    pub fn new(repo: Arc<dyn SkillRepository>) -> Self {
        Self { repo }
    }

    pub async fn get(&self, id: &str) -> Result<Skill, SkillCmdError> {
        self.repo.get(id).await?.filter(|s| !s.deleted).ok_or(SkillCmdError::NotFound)
    }

    pub async fn list(&self, project_id: &str) -> Result<Vec<Skill>, SkillCmdError> {
        Ok(self.repo.list_active(project_id).await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        id: &str,
        name: &str,
        description: &str,
        instructions: &str,
        includes: Vec<String>,
        enabled: bool,
    ) -> Result<Skill, SkillCmdError> {
        let mut s = self.get(id).await?;
        let trimmed = name.trim();
        if trimmed != s.name {
            if let Some(existing) = self.repo.find_active_by_name(&s.project_id, trimmed).await? {
                if existing.id != s.id {
                    return Err(SkillCmdError::NameExists);
                }
            }
            s.rename(name)?;
        }
        s.set_instructions(instructions)?;
        s.set_description(description);
        s.set_includes(includes);
        s.set_enabled(enabled);
        self.repo.save(&s).await?;
        Ok(s)
    }

    pub async fn delete(&self, id: &str) -> Result<(), SkillCmdError> {
        let mut s = self.get(id).await?;
        s.soft_delete();
        self.repo.save(&s).await?;
        Ok(())
    }

    pub async fn compose(
        &self,
        project_id: &str,
        skill_ids: &[String],
    ) -> Result<Composition, SkillCmdError> {
        let lib = SkillLibrary::new(self.repo.list_active(project_id).await?);
        lib.compose(skill_ids).map_err(SkillCmdError::Compose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemorySkillRepository;
    use crate::application::CreateSkillUseCase;

    async fn seeded() -> (SkillService, Arc<InMemorySkillRepository>) {
        let repo = Arc::new(InMemorySkillRepository::new());
        (SkillService::new(repo.clone()), repo)
    }

    #[tokio::test]
    async fn compose_via_service_expands_includes() {
        let (svc, repo) = seeded().await;
        let create = CreateSkillUseCase::new(repo);
        let a = create.execute("p1", "基础", "", "遵循六边形", &[]).await.expect("a").id;
        let b = create.execute("p1", "Rust", "", "用 thiserror", &[a.clone()]).await.expect("b").id;

        let comp = svc.compose("p1", &[b]).await.expect("compose");
        assert_eq!(comp.skill_ids, vec![a.clone(), comp.skill_ids[1].clone()]);
        assert!(comp.instructions.contains("遵循六边形"));
        assert!(comp.instructions.contains("用 thiserror"));
    }

    #[tokio::test]
    async fn compose_unknown_is_compose_error() {
        let (svc, _r) = seeded().await;
        assert!(matches!(
            svc.compose("p1", &["ghost".into()]).await.unwrap_err(),
            SkillCmdError::Compose(SkillError::UnknownInclude(_))
        ));
    }

    #[tokio::test]
    async fn update_and_delete() {
        let (svc, repo) = seeded().await;
        let id =
            CreateSkillUseCase::new(repo).execute("p1", "n", "", "i", &[]).await.expect("c").id;
        let u = svc.update(&id, "n2", "d", "i2", vec![], false).await.expect("u");
        assert_eq!(u.name, "n2");
        assert!(!u.enabled);
        svc.delete(&id).await.expect("del");
        assert_eq!(svc.get(&id).await.unwrap_err(), SkillCmdError::NotFound);
    }
}

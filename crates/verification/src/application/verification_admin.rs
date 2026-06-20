//! 用例:验证编排 —— get / report / link(建覆盖)/ sync(同步任务交付验证状态)。

use std::sync::Arc;

use crate::domain::{CompletenessReport, Verification, VerificationError};
use crate::ports::{RepoError, VerificationRepository};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationCmdError {
    /// 验证不存在 → 404。
    NotFound,
    /// 标准索引不存在 → 404。
    NoSuchCriterion(u32),
    /// 入参校验失败 → 400。
    Validation(VerificationError),
    /// 存储错误 → 500。
    Repo(RepoError),
}

impl From<RepoError> for VerificationCmdError {
    fn from(e: RepoError) -> Self {
        Self::Repo(e)
    }
}

impl From<VerificationError> for VerificationCmdError {
    fn from(e: VerificationError) -> Self {
        match e {
            VerificationError::NoSuchCriterion(i) => Self::NoSuchCriterion(i),
            other => Self::Validation(other),
        }
    }
}

#[derive(Clone)]
pub struct VerificationService {
    repo: Arc<dyn VerificationRepository>,
}

impl VerificationService {
    pub fn new(repo: Arc<dyn VerificationRepository>) -> Self {
        Self { repo }
    }

    pub async fn get(&self, id: &str) -> Result<Verification, VerificationCmdError> {
        self.repo.get(id).await?.ok_or(VerificationCmdError::NotFound)
    }

    pub async fn report(&self, id: &str) -> Result<CompletenessReport, VerificationCmdError> {
        Ok(self.get(id).await?.report())
    }

    /// 按需求版本查找验证(编排器用:据交付定位要回灌的验证)。
    pub async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Verification>, VerificationCmdError> {
        Ok(self.repo.find_by_requirement_version(requirement_id, requirement_version).await?)
    }

    /// 建立覆盖链:某任务覆盖某条验收标准(需求 → 任务 追溯)。
    pub async fn link(
        &self,
        id: &str,
        criterion_index: u32,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Verification, VerificationCmdError> {
        let mut v = self.get(id).await?;
        v.link(criterion_index, decomposition_id, task_id)?;
        self.repo.save(&v).await?;
        Ok(v)
    }

    /// 按验收标准**文本**自动建立覆盖链(供编排器据任务 `acceptance_criteria` 自动关联)。
    /// 对每条匹配到的标准都 link 该任务(幂等);返回更新后的聚合。
    pub async fn link_by_criteria_texts(
        &self,
        id: &str,
        decomposition_id: &str,
        task_id: &str,
        texts: &[String],
    ) -> Result<Verification, VerificationCmdError> {
        let mut v = self.get(id).await?;
        v.link_task_by_texts(decomposition_id, task_id, texts);
        self.repo.save(&v).await?;
        Ok(v)
    }

    /// 同步任务的交付/验证状态到其覆盖链(任务 → 实现 追溯;`satisfied` 通常来自 delivery 验证)。
    pub async fn sync_task(
        &self,
        id: &str,
        decomposition_id: &str,
        task_id: &str,
        satisfied: bool,
    ) -> Result<Verification, VerificationCmdError> {
        let mut v = self.get(id).await?;
        v.sync_task(decomposition_id, task_id, satisfied);
        self.repo.save(&v).await?;
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryVerificationRepository;
    use crate::application::CreateVerificationUseCase;
    use crate::domain::{CriterionStatus, GapKind};

    async fn seeded() -> (VerificationService, String) {
        let repo = Arc::new(InMemoryVerificationRepository::new());
        let id = CreateVerificationUseCase::new(repo.clone())
            .execute("req1", 1, &["登录成功".into(), "错误密码拒绝".into()])
            .await
            .expect("seed")
            .id;
        (VerificationService::new(repo), id)
    }

    #[tokio::test]
    async fn link_sync_and_report_closes_gaps() {
        let (svc, id) = seeded().await;
        // 初始两条都是 Uncovered 缺口
        let r = svc.report(&id).await.expect("report");
        assert!(!r.complete);
        assert_eq!(r.gaps.len(), 2);

        // 覆盖两条标准
        svc.link(&id, 0, "d1", "t1").await.expect("link0");
        svc.link(&id, 1, "d1", "t2").await.expect("link1");
        let r = svc.report(&id).await.expect("report");
        // 覆盖未验证 → Unverified 缺口
        assert_eq!(r.gaps.iter().filter(|g| g.kind == GapKind::Unverified).count(), 2);

        // 同步任务验证通过
        svc.sync_task(&id, "d1", "t1", true).await.expect("sync");
        svc.sync_task(&id, "d1", "t2", true).await.expect("sync");
        let r = svc.report(&id).await.expect("report");
        assert!(r.complete);
        assert_eq!(r.satisfied, 2);
        assert!(r.criteria.iter().all(|c| c.status == CriterionStatus::Satisfied));
    }

    #[tokio::test]
    async fn link_by_criteria_texts_then_sync_completes() {
        let (svc, id) = seeded().await; // 标准: 登录成功 / 错误密码拒绝
        // 按文本自动关联两个任务(模拟编排器据 acceptance_criteria 关联)
        svc.link_by_criteria_texts(&id, "d1", "t1", &["登录成功".into()]).await.expect("link0");
        svc.link_by_criteria_texts(&id, "d1", "t2", &["错误密码拒绝".into()]).await.expect("link1");
        let r = svc.report(&id).await.expect("report");
        // 已覆盖未验证
        assert_eq!(r.gaps.iter().filter(|g| g.kind == GapKind::Unverified).count(), 2);
        // 同步验证通过 → complete
        svc.sync_task(&id, "d1", "t1", true).await.expect("sync0");
        svc.sync_task(&id, "d1", "t2", true).await.expect("sync1");
        assert!(svc.report(&id).await.expect("report").complete);
    }

    #[tokio::test]
    async fn link_unknown_criterion_is_404() {
        let (svc, id) = seeded().await;
        assert_eq!(
            svc.link(&id, 9, "d1", "t1").await.unwrap_err(),
            VerificationCmdError::NoSuchCriterion(9)
        );
    }

    #[tokio::test]
    async fn missing_verification_is_not_found() {
        let (svc, _id) = seeded().await;
        assert_eq!(svc.get("ghost").await.unwrap_err(), VerificationCmdError::NotFound);
    }
}

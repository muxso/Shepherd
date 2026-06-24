//! 用例:提案的 Design 阶段编排 —— 起草 / 提交设计稿 / 审批门 / 驳回修订。

use std::sync::Arc;

use crate::domain::{Proposal, ProposalError};
use crate::ports::{DesignDrafter, ProposalRepository, RepoError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalCmdError {
    /// 提案不存在 → 404。
    NotFound,
    /// 入参非法 → 400。
    Validation(String),
    /// 非法状态流转 → 409。
    Conflict(ProposalError),
    /// 存储错误 → 500。
    Repo(RepoError),
}

impl From<RepoError> for ProposalCmdError {
    fn from(e: RepoError) -> Self {
        Self::Repo(e)
    }
}
impl From<ProposalError> for ProposalCmdError {
    fn from(e: ProposalError) -> Self {
        match e {
            ProposalError::Empty(f) => Self::Validation(format!("empty {f}")),
            ProposalError::CommentRequired => Self::Validation("review comment required".into()),
            other => Self::Conflict(other),
        }
    }
}

#[derive(Clone)]
pub struct ProposalService {
    repo: Arc<dyn ProposalRepository>,
    /// 可选起草执行者(AI agent):配置后 `create` 即派发起草,跑完经 submit_design 回填。
    drafter: Option<Arc<dyn DesignDrafter>>,
}

impl ProposalService {
    pub fn new(repo: Arc<dyn ProposalRepository>) -> Self {
        Self { repo, drafter: None }
    }

    /// 挂上起草执行者(人机协同):新建提案即自动派发 agent 起草设计稿。
    pub fn with_drafter(mut self, drafter: Arc<dyn DesignDrafter>) -> Self {
        self.drafter = Some(drafter);
        self
    }

    /// 为某需求开一份设计提案(起草态)。配了 drafter 则顺手派发 agent 起草(尽力而为)。
    pub async fn create(
        &self,
        requirement_id: &str,
        title: &str,
    ) -> Result<Proposal, ProposalCmdError> {
        if requirement_id.trim().is_empty() {
            return Err(ProposalCmdError::Validation("requirementId required".into()));
        }
        if title.trim().is_empty() {
            return Err(ProposalCmdError::Validation("title required".into()));
        }
        let p = self.repo.create(requirement_id.trim(), title.trim()).await?;
        // 派发起草:失败不阻断建单(提案保持 Drafting,人仍可手填设计稿)。
        if let Some(d) = &self.drafter {
            let _ = d.request_draft(&p).await;
        }
        Ok(p)
    }

    /// 提交设计稿(agent 产出 / 人填写)→ 进入待审。
    pub async fn submit_design(&self, id: &str, doc: &str) -> Result<Proposal, ProposalCmdError> {
        let mut p = self.get(id).await?;
        p.submit_design(doc)?;
        self.repo.save(&p).await?;
        Ok(p)
    }

    /// 审批通过(门)→ 放行拆分。
    pub async fn approve(&self, id: &str) -> Result<Proposal, ProposalCmdError> {
        let mut p = self.get(id).await?;
        p.approve()?;
        self.repo.save(&p).await?;
        Ok(p)
    }

    /// 驳回带反馈 → 进入修订(下一轮 submit_design 会带着该意见重做)。
    pub async fn request_changes(
        &self,
        id: &str,
        comment: &str,
    ) -> Result<Proposal, ProposalCmdError> {
        let mut p = self.get(id).await?;
        p.request_changes(comment)?;
        self.repo.save(&p).await?;
        Ok(p)
    }

    pub async fn get(&self, id: &str) -> Result<Proposal, ProposalCmdError> {
        self.repo.get(id).await?.ok_or(ProposalCmdError::NotFound)
    }

    pub async fn list_by_requirement(
        &self,
        requirement_id: &str,
    ) -> Result<Vec<Proposal>, ProposalCmdError> {
        Ok(self.repo.list_by_requirement(requirement_id).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryProposalRepository;
    use crate::domain::ProposalStatus;

    fn svc() -> ProposalService {
        ProposalService::new(Arc::new(InMemoryProposalRepository::new()))
    }

    #[tokio::test]
    async fn full_lifecycle_with_revision() {
        let s = svc();
        let p = s.create("req-1", "登录改造").await.expect("create");
        assert_eq!(p.status, ProposalStatus::Drafting);

        // agent 提交 v1 → 待审
        let p = s.submit_design(&p.id, "v1").await.expect("submit");
        assert_eq!(p.status, ProposalStatus::PendingReview);

        // 人驳回 → 修订
        let p = s.request_changes(&p.id, "补充失败分支").await.expect("reject");
        assert_eq!(p.status, ProposalStatus::ChangesRequested);
        assert_eq!(p.revision, 1);

        // agent 据反馈提交 v2 → 待审 → 批准
        let p = s.submit_design(&p.id, "v2").await.expect("submit2");
        assert_eq!(p.status, ProposalStatus::PendingReview);
        let p = s.approve(&p.id).await.expect("approve");
        assert_eq!(p.status, ProposalStatus::Approved);
    }

    #[tokio::test]
    async fn validation_and_conflict_errors() {
        let s = svc();
        assert!(matches!(s.create("", "t").await, Err(ProposalCmdError::Validation(_))));
        let p = s.create("r", "t").await.expect("create");
        // 起草态批准 → 冲突(非法流转)
        assert!(matches!(s.approve(&p.id).await, Err(ProposalCmdError::Conflict(_))));
        // 不存在 → 404
        assert_eq!(s.get("ghost").await.unwrap_err(), ProposalCmdError::NotFound);
    }

    #[tokio::test]
    async fn create_triggers_agent_draft_then_callback_loop() {
        use crate::domain::Proposal;
        use crate::ports::{DesignDrafter, DraftError};
        use async_trait::async_trait;
        use std::sync::Mutex;

        // 记录被派发起草的提案(模拟 AI agent 受理)。
        #[derive(Default)]
        struct RecordingDrafter {
            drafted: Mutex<Vec<String>>,
        }
        #[async_trait]
        impl DesignDrafter for RecordingDrafter {
            async fn request_draft(&self, p: &Proposal) -> Result<(), DraftError> {
                self.drafted.lock().unwrap().push(p.id.clone());
                Ok(())
            }
        }

        let drafter = Arc::new(RecordingDrafter::default());
        let s = ProposalService::new(Arc::new(InMemoryProposalRepository::new()))
            .with_drafter(drafter.clone());

        // 建单即派发 agent 起草。
        let p = s.create("req-9", "支付重构").await.expect("create");
        assert_eq!(drafter.drafted.lock().unwrap().as_slice(), &[p.id.clone()]);
        assert_eq!(p.status, ProposalStatus::Drafting); // 仍待 agent 回填

        // 模拟 agent 跑完回调 /proposal/{id}/design → 进入待审。
        let p = s.submit_design(&p.id, "## 架构\n拆分支付域").await.expect("callback");
        assert_eq!(p.status, ProposalStatus::PendingReview);
        assert_eq!(p.design_doc.as_deref(), Some("## 架构\n拆分支付域"));
    }

    #[tokio::test]
    async fn lists_by_requirement() {
        let s = svc();
        s.create("r1", "a").await.expect("c");
        s.create("r1", "b").await.expect("c");
        s.create("r2", "c").await.expect("c");
        assert_eq!(s.list_by_requirement("r1").await.expect("l").len(), 2);
        assert_eq!(s.list_by_requirement("r2").await.expect("l").len(), 1);
    }
}

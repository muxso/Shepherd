use std::sync::Arc;

use crate::domain::{Proposal, ProposalError};
use crate::ports::{BreakdownTrigger, DesignDrafter, ProposalRepository, RepoError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalCmdError {
    NotFound,
    Validation(String),
    Conflict(ProposalError),
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
    drafter: Option<Arc<dyn DesignDrafter>>,
    breakdown: Option<Arc<dyn BreakdownTrigger>>,
}

impl ProposalService {
    pub fn new(repo: Arc<dyn ProposalRepository>) -> Self {
        Self { repo, drafter: None, breakdown: None }
    }

    pub fn with_drafter(mut self, drafter: Arc<dyn DesignDrafter>) -> Self {
        self.drafter = Some(drafter);
        self
    }

    pub fn with_breakdown_trigger(mut self, breakdown: Arc<dyn BreakdownTrigger>) -> Self {
        self.breakdown = Some(breakdown);
        self
    }

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
        if let Some(d) = &self.drafter {
            let _ = d.request_draft(&p).await;
        }
        Ok(p)
    }

    pub async fn submit_design(&self, id: &str, doc: &str) -> Result<Proposal, ProposalCmdError> {
        let mut p = self.get(id).await?;
        p.submit_design(doc)?;
        self.repo.save(&p).await?;
        Ok(p)
    }

    pub async fn approve(&self, id: &str) -> Result<Proposal, ProposalCmdError> {
        let mut p = self.get(id).await?;
        p.approve()?;
        self.repo.save(&p).await?;
        if let Some(b) = &self.breakdown {
            let _ = b.on_design_approved(&p).await;
        }
        Ok(p)
    }

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

        let p = s.submit_design(&p.id, "v1").await.expect("submit");
        assert_eq!(p.status, ProposalStatus::PendingReview);

        let p = s.request_changes(&p.id, "补充失败分支").await.expect("reject");
        assert_eq!(p.status, ProposalStatus::ChangesRequested);
        assert_eq!(p.revision, 1);

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
        assert!(matches!(s.approve(&p.id).await, Err(ProposalCmdError::Conflict(_))));
        assert_eq!(s.get("ghost").await.unwrap_err(), ProposalCmdError::NotFound);
    }

    #[tokio::test]
    async fn create_triggers_agent_draft_then_callback_loop() {
        use crate::domain::Proposal;
        use crate::ports::{DesignDrafter, DraftError};
        use async_trait::async_trait;
        use std::sync::Mutex;

        #[derive(Default)]
        struct RecordingDrafter {
            drafted: Mutex<Vec<String>>,
        }
        #[async_trait]
        impl DesignDrafter for RecordingDrafter {
            async fn request_draft(&self, p: &Proposal) -> Result<(), DraftError> {
                self.drafted.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(p.id.clone());
                Ok(())
            }
        }

        let drafter = Arc::new(RecordingDrafter::default());
        let s = ProposalService::new(Arc::new(InMemoryProposalRepository::new()))
            .with_drafter(drafter.clone());

        let p = s.create("req-9", "支付重构").await.expect("create");
        assert_eq!(drafter.drafted.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_slice(), &[p.id.clone()]);
        assert_eq!(p.status, ProposalStatus::Drafting);

        let p = s.submit_design(&p.id, "## 架构\n拆分支付域").await.expect("callback");
        assert_eq!(p.status, ProposalStatus::PendingReview);
        assert_eq!(p.design_doc.as_deref(), Some("## 架构\n拆分支付域"));
    }

    #[tokio::test]
    async fn approve_fires_breakdown_trigger_once() {
        use crate::domain::Proposal;
        use crate::ports::{BreakdownTrigger, TriggerError};
        use async_trait::async_trait;
        use std::sync::Mutex;

        #[derive(Default)]
        struct RecordingTrigger {
            fired: Mutex<Vec<String>>,
        }
        #[async_trait]
        impl BreakdownTrigger for RecordingTrigger {
            async fn on_design_approved(&self, p: &Proposal) -> Result<(), TriggerError> {
                self.fired.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(p.requirement_id.clone());
                Ok(())
            }
        }

        let trig = Arc::new(RecordingTrigger::default());
        let s = ProposalService::new(Arc::new(InMemoryProposalRepository::new()))
            .with_breakdown_trigger(trig.clone());
        let p = s.create("req-7", "设计").await.expect("create");
        s.submit_design(&p.id, "doc").await.expect("submit");

        s.approve(&p.id).await.expect("approve");
        assert_eq!(trig.fired.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_slice(), &["req-7".to_string()]);

        assert!(s.approve(&p.id).await.is_err());
        assert_eq!(trig.fired.lock().unwrap_or_else(std::sync::PoisonError::into_inner).len(), 1);
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

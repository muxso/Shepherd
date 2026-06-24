//! 变更提案聚合(OpenSpec / BMAD 风格的「Design 阶段 + 审批门」)。
//!
//! 在 Shepherd 的「需求 → 拆分 → 派发 → 验证」链路里,本聚合插在**拆分之前**:
//! agent(或人)产出**设计稿** → 人**审批门**(批准 / 驳回带反馈)→ 驳回则进入**修订循环**
//! (回到起草,带上评审意见重做)→ 批准后方可进入拆分实现。
//!
//! 状态机:`Drafting →(submit_design)→ PendingReview →(approve)→ Approved`
//!                                          └(request_changes)→ ChangesRequested →(submit_design)→ …

use thiserror::Error;

/// 提案生命周期。`Approved` 为终态(放行拆分)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus {
    /// 等待设计稿(agent 起草中 / 待提交)。
    Drafting,
    /// 设计稿就绪,等人审批(门)。
    PendingReview,
    /// 审批通过 → 放行拆分/实现。
    Approved,
    /// 驳回(带反馈)→ 回到起草做修订。
    ChangesRequested,
}

impl ProposalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Drafting => "DRAFTING",
            Self::PendingReview => "PENDING_REVIEW",
            Self::Approved => "APPROVED",
            Self::ChangesRequested => "CHANGES_REQUESTED",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Approved)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProposalError {
    #[error("empty {0}")]
    Empty(&'static str),
    #[error("invalid transition from {from} to {to}")]
    Transition { from: &'static str, to: &'static str },
    #[error("review comment required when requesting changes")]
    CommentRequired,
}

/// 一份变更提案:绑定某需求,承载设计稿 + 审批状态 + 修订轮次。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub id: String,
    pub requirement_id: String,
    pub title: String,
    /// 当前设计稿(markdown);未提交则 None。
    pub design_doc: Option<String>,
    pub status: ProposalStatus,
    /// 最近一次驳回的评审意见(反馈给 agent 修订)。
    pub review_comment: Option<String>,
    /// 修订轮次:每驳回一次 +1。
    pub revision: u32,
}

impl Proposal {
    pub fn new(id: &str, requirement_id: &str, title: &str) -> Result<Self, ProposalError> {
        if id.trim().is_empty() {
            return Err(ProposalError::Empty("id"));
        }
        if requirement_id.trim().is_empty() {
            return Err(ProposalError::Empty("requirement_id"));
        }
        if title.trim().is_empty() {
            return Err(ProposalError::Empty("title"));
        }
        Ok(Self {
            id: id.trim().to_string(),
            requirement_id: requirement_id.trim().to_string(),
            title: title.trim().to_string(),
            design_doc: None,
            status: ProposalStatus::Drafting,
            review_comment: None,
            revision: 0,
        })
    }

    /// 提交设计稿(agent/人):`Drafting | ChangesRequested → PendingReview`。
    pub fn submit_design(&mut self, doc: &str) -> Result<(), ProposalError> {
        if doc.trim().is_empty() {
            return Err(ProposalError::Empty("design_doc"));
        }
        match self.status {
            ProposalStatus::Drafting | ProposalStatus::ChangesRequested => {
                self.design_doc = Some(doc.trim().to_string());
                self.review_comment = None;
                self.status = ProposalStatus::PendingReview;
                Ok(())
            }
            other => Err(ProposalError::Transition { from: other.as_str(), to: "PENDING_REVIEW" }),
        }
    }

    /// 审批通过(门):`PendingReview → Approved`。
    pub fn approve(&mut self) -> Result<(), ProposalError> {
        match self.status {
            ProposalStatus::PendingReview => {
                self.status = ProposalStatus::Approved;
                Ok(())
            }
            other => Err(ProposalError::Transition { from: other.as_str(), to: "APPROVED" }),
        }
    }

    /// 驳回(带反馈):`PendingReview → ChangesRequested`,revision+1。
    pub fn request_changes(&mut self, comment: &str) -> Result<(), ProposalError> {
        if comment.trim().is_empty() {
            return Err(ProposalError::CommentRequired);
        }
        match self.status {
            ProposalStatus::PendingReview => {
                self.review_comment = Some(comment.trim().to_string());
                self.revision += 1;
                self.status = ProposalStatus::ChangesRequested;
                Ok(())
            }
            other => {
                Err(ProposalError::Transition { from: other.as_str(), to: "CHANGES_REQUESTED" })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Proposal {
        Proposal::new("p1", "req-1", "登录改造设计").expect("new")
    }

    #[test]
    fn new_validates_and_starts_drafting() {
        let p = draft();
        assert_eq!(p.status, ProposalStatus::Drafting);
        assert_eq!(p.revision, 0);
        assert!(p.design_doc.is_none());
        assert!(matches!(Proposal::new("", "r", "t"), Err(ProposalError::Empty("id"))));
        assert!(matches!(Proposal::new("i", "", "t"), Err(ProposalError::Empty("requirement_id"))));
        assert!(matches!(Proposal::new("i", "r", "  "), Err(ProposalError::Empty("title"))));
    }

    #[test]
    fn happy_path_draft_review_approve() {
        let mut p = draft();
        p.submit_design("## 方案\n用 JWT").expect("submit");
        assert_eq!(p.status, ProposalStatus::PendingReview);
        assert_eq!(p.design_doc.as_deref(), Some("## 方案\n用 JWT"));
        p.approve().expect("approve");
        assert_eq!(p.status, ProposalStatus::Approved);
        assert!(p.status.is_terminal());
    }

    #[test]
    fn reject_then_revise_loop() {
        let mut p = draft();
        p.submit_design("v1").expect("submit v1");
        p.request_changes("缺少错误码设计").expect("reject");
        assert_eq!(p.status, ProposalStatus::ChangesRequested);
        assert_eq!(p.revision, 1);
        assert_eq!(p.review_comment.as_deref(), Some("缺少错误码设计"));
        // 修订:重新提交 → 回到待审,评审意见清空。
        p.submit_design("v2 含错误码").expect("submit v2");
        assert_eq!(p.status, ProposalStatus::PendingReview);
        assert!(p.review_comment.is_none());
        assert_eq!(p.design_doc.as_deref(), Some("v2 含错误码"));
        p.approve().expect("approve");
        assert_eq!(p.status, ProposalStatus::Approved);
        assert_eq!(p.revision, 1); // 修订计数保留
    }

    #[test]
    fn illegal_transitions_rejected() {
        let mut p = draft();
        // 起草态不能直接批准。
        assert!(matches!(p.approve(), Err(ProposalError::Transition { to: "APPROVED", .. })));
        // 起草态不能驳回。
        assert!(matches!(
            p.request_changes("x"),
            Err(ProposalError::Transition { to: "CHANGES_REQUESTED", .. })
        ));
        // 空设计稿 / 空评审意见。
        assert!(matches!(p.submit_design("  "), Err(ProposalError::Empty("design_doc"))));
        p.submit_design("d").expect("submit");
        assert!(matches!(p.request_changes(""), Err(ProposalError::CommentRequired)));
        // 已批准后不可再改。
        p.approve().expect("approve");
        assert!(matches!(p.submit_design("d2"), Err(ProposalError::Transition { from: "APPROVED", .. })));
    }
}

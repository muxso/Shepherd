use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus {
    Drafting,
    PendingReview,
    Approved,
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

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "DRAFTING" => Some(Self::Drafting),
            "PENDING_REVIEW" => Some(Self::PendingReview),
            "APPROVED" => Some(Self::Approved),
            "CHANGES_REQUESTED" => Some(Self::ChangesRequested),
            _ => None,
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub id: String,
    pub requirement_id: String,
    pub title: String,
    pub design_doc: Option<String>,
    pub status: ProposalStatus,
    pub review_comment: Option<String>,
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

    pub fn approve(&mut self) -> Result<(), ProposalError> {
        match self.status {
            ProposalStatus::PendingReview => {
                self.status = ProposalStatus::Approved;
                Ok(())
            }
            other => Err(ProposalError::Transition { from: other.as_str(), to: "APPROVED" }),
        }
    }

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
        Proposal::new("p1", "req-1", "login revamp design").expect("new")
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
        p.submit_design("## proposal\nuses JWT").expect("submit");
        assert_eq!(p.status, ProposalStatus::PendingReview);
        assert_eq!(p.design_doc.as_deref(), Some("## proposal\nuses JWT"));
        p.approve().expect("approve");
        assert_eq!(p.status, ProposalStatus::Approved);
        assert!(p.status.is_terminal());
    }

    #[test]
    fn reject_then_revise_loop() {
        let mut p = draft();
        p.submit_design("v1").expect("submit v1");
        p.request_changes("missing error code design").expect("reject");
        assert_eq!(p.status, ProposalStatus::ChangesRequested);
        assert_eq!(p.revision, 1);
        assert_eq!(p.review_comment.as_deref(), Some("missing error code design"));
        p.submit_design("v2 with error codes").expect("submit v2");
        assert_eq!(p.status, ProposalStatus::PendingReview);
        assert!(p.review_comment.is_none());
        assert_eq!(p.design_doc.as_deref(), Some("v2 with error codes"));
        p.approve().expect("approve");
        assert_eq!(p.status, ProposalStatus::Approved);
        assert_eq!(p.revision, 1);
    }

    #[test]
    fn illegal_transitions_rejected() {
        let mut p = draft();
        assert!(matches!(p.approve(), Err(ProposalError::Transition { to: "APPROVED", .. })));
        assert!(matches!(
            p.request_changes("x"),
            Err(ProposalError::Transition { to: "CHANGES_REQUESTED", .. })
        ));
        assert!(matches!(p.submit_design("  "), Err(ProposalError::Empty("design_doc"))));
        p.submit_design("d").expect("submit");
        assert!(matches!(p.request_changes(""), Err(ProposalError::CommentRequired)));
        p.approve().expect("approve");
        assert!(matches!(
            p.submit_design("d2"),
            Err(ProposalError::Transition { from: "APPROVED", .. })
        ));
    }
}

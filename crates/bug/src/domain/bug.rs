use crate::domain::status_flow::StatusFlowGraph;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BugError {
    #[error("bug title must not be empty")]
    EmptyTitle,
    #[error("unknown target status: {0}")]
    UnknownStatus(String),
    #[error("transition not allowed: {from} -> {to}")]
    TransitionNotAllowed { from: String, to: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBug {
    pub project_id: String,
    pub title: String,
    pub created_by: Option<String>,
}

impl NewBug {
    pub fn new(project_id: &str, title: &str) -> Result<Self, BugError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(BugError::EmptyTitle);
        }
        Ok(Self { project_id: project_id.to_string(), title: title.to_string(), created_by: None })
    }

    pub fn with_created_by(mut self, user_id: Option<&str>) -> Self {
        self.created_by = user_id.map(|s| s.to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bug {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub deleted: bool,
    pub created_at: i64,
    pub created_by: Option<String>,
}

impl Bug {
    pub fn change_status(&mut self, to: &str, flow: &StatusFlowGraph) -> Result<(), BugError> {
        if !flow.contains(to) {
            return Err(BugError::UnknownStatus(to.to_string()));
        }
        if !flow.can_transition(&self.status, to) {
            return Err(BugError::TransitionNotAllowed {
                from: self.status.clone(),
                to: to.to_string(),
            });
        }
        self.status = to.to_string();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::status_flow::StatusItem;

    fn flow() -> StatusFlowGraph {
        StatusFlowGraph::new(
            vec![
                StatusItem::new("NEW", "新建", true),
                StatusItem::new("RESOLVED", "已解决", true),
                StatusItem::new("CLOSED", "已关闭", true),
            ],
            vec![("NEW".into(), "RESOLVED".into()), ("RESOLVED".into(), "CLOSED".into())],
        )
    }

    fn bug_at(status: &str) -> Bug {
        Bug {
            id: "b1".into(),
            project_id: "p1".into(),
            title: "boom".into(),
            status: status.into(),
            deleted: false,
            created_at: 0,
            created_by: None,
        }
    }

    #[test]
    fn new_bug_rejects_blank_title() {
        assert_eq!(NewBug::new("p1", "  "), Err(BugError::EmptyTitle));
    }

    #[test]
    fn new_bug_carries_created_by() {
        let nb = NewBug::new("p1", "boom").expect("valid").with_created_by(Some("alice"));
        assert_eq!(nb.created_by.as_deref(), Some("alice"));
        assert_eq!(NewBug::new("p1", "boom").expect("valid").created_by, None);
    }

    #[test]
    fn valid_transition_updates_status() {
        let mut b = bug_at("NEW");
        b.change_status("RESOLVED", &flow()).expect("allowed");
        assert_eq!(b.status, "RESOLVED");
    }

    #[test]
    fn disallowed_transition_is_rejected_and_status_unchanged() {
        let mut b = bug_at("NEW");
        let err = b.change_status("CLOSED", &flow()).unwrap_err();
        assert_eq!(err, BugError::TransitionNotAllowed { from: "NEW".into(), to: "CLOSED".into() });
        assert_eq!(b.status, "NEW");
    }

    #[test]
    fn unknown_target_status_is_rejected() {
        let mut b = bug_at("NEW");
        let err = b.change_status("GHOST", &flow()).unwrap_err();
        assert_eq!(err, BugError::UnknownStatus("GHOST".into()));
        assert_eq!(b.status, "NEW");
    }
}

//! 缺陷领域模型 + 状态流转规则。

use crate::domain::status_flow::StatusFlowGraph;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BugError {
    #[error("bug title must not be empty")]
    EmptyTitle,
    /// 目标状态不在该项目的状态流图里。
    #[error("unknown target status: {0}")]
    UnknownStatus(String),
    /// 当前状态到目标状态没有配置流转边。
    #[error("transition not allowed: {from} -> {to}")]
    TransitionNotAllowed { from: String, to: String },
}

/// 创建缺陷的入站请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBug {
    pub project_id: String,
    pub title: String,
}

impl NewBug {
    pub fn new(project_id: &str, title: &str) -> Result<Self, BugError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(BugError::EmptyTitle);
        }
        Ok(Self { project_id: project_id.to_string(), title: title.to_string() })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bug {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub deleted: bool,
}

impl Bug {
    /// 依据状态流图校验并执行流转。非法流转不改变自身状态。
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
        }
    }

    #[test]
    fn new_bug_rejects_blank_title() {
        assert_eq!(NewBug::new("p1", "  "), Err(BugError::EmptyTitle));
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
        let err = b.change_status("CLOSED", &flow()).unwrap_err(); // 不能跳过 RESOLVED
        assert_eq!(err, BugError::TransitionNotAllowed { from: "NEW".into(), to: "CLOSED".into() });
        assert_eq!(b.status, "NEW"); // 未变
    }

    #[test]
    fn unknown_target_status_is_rejected() {
        let mut b = bug_at("NEW");
        let err = b.change_status("GHOST", &flow()).unwrap_err();
        assert_eq!(err, BugError::UnknownStatus("GHOST".into()));
        assert_eq!(b.status, "NEW");
    }
}

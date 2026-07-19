use std::sync::Arc;

use thiserror::Error;

use crate::domain::{render_template, NoticeError, Robot, RobotDraft, Rule, RuleDraft};
use crate::ports::{NoticeRuleStore, RepoError, RobotDelivery, RobotSender};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleAdminError {
    #[error("{0}")]
    Invalid(NoticeError),
    #[error("not found")]
    NotFound,
    #[error("storage backend error: {0}")]
    Backend(String),
    #[error("webhook delivery failed: {0}")]
    Delivery(String),
}

impl From<RepoError> for RuleAdminError {
    fn from(e: RepoError) -> Self {
        let RepoError::Backend(m) = e;
        RuleAdminError::Backend(m)
    }
}

/// Robot / rule management plus the "send a test message" probe.
#[derive(Clone)]
pub struct NoticeRuleAdmin {
    store: Arc<dyn NoticeRuleStore>,
    sender: Arc<dyn RobotSender>,
}

impl NoticeRuleAdmin {
    pub fn new(store: Arc<dyn NoticeRuleStore>, sender: Arc<dyn RobotSender>) -> Self {
        Self { store, sender }
    }

    pub async fn create_robot(&self, draft: RobotDraft) -> Result<Robot, RuleAdminError> {
        let draft = draft.validated().map_err(RuleAdminError::Invalid)?;
        Ok(self.store.insert_robot(&draft).await?)
    }

    pub async fn update_robot(&self, id: &str, draft: RobotDraft) -> Result<Robot, RuleAdminError> {
        let draft = draft.validated().map_err(RuleAdminError::Invalid)?;
        self.store.update_robot(id, &draft).await?.ok_or(RuleAdminError::NotFound)
    }

    pub async fn delete_robot(&self, id: &str, project_id: &str) -> Result<(), RuleAdminError> {
        match self.store.delete_robot(id, project_id).await? {
            true => Ok(()),
            false => Err(RuleAdminError::NotFound),
        }
    }

    pub async fn list_robots(&self, project_id: &str) -> Result<Vec<Robot>, RuleAdminError> {
        Ok(self.store.list_robots(project_id).await?)
    }

    /// Sends a test message to the robot and returns the upstream response.
    pub async fn test_robot(
        &self,
        id: &str,
        project_id: &str,
        operator: &str,
    ) -> Result<RobotDelivery, RuleAdminError> {
        let robot = self.store.get_robot(id, project_id).await?.ok_or(RuleAdminError::NotFound)?;
        let text = render_template("", "测试消息 (test message)", operator, &now_text());
        self.sender.send(&robot, &text).await.map_err(RuleAdminError::Delivery)
    }

    pub async fn create_rule(&self, draft: RuleDraft) -> Result<Rule, RuleAdminError> {
        let draft = draft.validated().map_err(RuleAdminError::Invalid)?;
        Ok(self.store.insert_rule(&draft).await?)
    }

    pub async fn update_rule(&self, id: &str, draft: RuleDraft) -> Result<Rule, RuleAdminError> {
        let draft = draft.validated().map_err(RuleAdminError::Invalid)?;
        self.store.update_rule(id, &draft).await?.ok_or(RuleAdminError::NotFound)
    }

    pub async fn delete_rule(&self, id: &str, project_id: &str) -> Result<(), RuleAdminError> {
        match self.store.delete_rule(id, project_id).await? {
            true => Ok(()),
            false => Err(RuleAdminError::NotFound),
        }
    }

    pub async fn list_rules(&self, project_id: &str) -> Result<Vec<Rule>, RuleAdminError> {
        Ok(self.store.list_rules(project_id).await?)
    }
}

/// Local wall-clock time for the `${time}` placeholder.
pub(crate) fn now_text() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::in_memory::{InMemoryRuleStore, RecordingRobotSender};
    use crate::domain::{Channel, Platform};

    fn robot_draft(name: &str) -> RobotDraft {
        RobotDraft {
            project_id: "p1".into(),
            name: name.into(),
            platform: Platform::Wecom,
            webhook_url: "https://hook".into(),
            secret: String::new(),
            enabled: true,
        }
    }

    fn admin() -> (Arc<InMemoryRuleStore>, Arc<RecordingRobotSender>, NoticeRuleAdmin) {
        let store = Arc::new(InMemoryRuleStore::new());
        let sender = Arc::new(RecordingRobotSender::new());
        (store.clone(), sender.clone(), NoticeRuleAdmin::new(store, sender))
    }

    #[tokio::test]
    async fn robot_crud_roundtrip() {
        let (_s, _tx, admin) = admin();
        let r = admin.create_robot(robot_draft("bot")).await.expect("create");
        assert_eq!(r.name, "bot");
        let mut d = robot_draft("bot2");
        d.enabled = false;
        let r2 = admin.update_robot(&r.id, d).await.expect("update");
        assert_eq!((r2.name.as_str(), r2.enabled), ("bot2", false));
        assert_eq!(admin.list_robots("p1").await.expect("list").len(), 1);
        // Wrong project scope: not found.
        let mut d = robot_draft("x");
        d.project_id = "p2".into();
        assert_eq!(admin.update_robot(&r.id, d).await, Err(RuleAdminError::NotFound));
        assert_eq!(admin.delete_robot(&r.id, "p2").await, Err(RuleAdminError::NotFound));
        admin.delete_robot(&r.id, "p1").await.expect("delete");
        assert!(admin.list_robots("p1").await.expect("list").is_empty());
    }

    #[tokio::test]
    async fn test_robot_sends_through_the_webhook() {
        let (_s, tx, admin) = admin();
        let r = admin.create_robot(robot_draft("bot")).await.expect("create");
        let d = admin.test_robot(&r.id, "p1", "admin").await.expect("test");
        assert_eq!(d.status, 200);
        let sent = tx.sent();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].1.contains("admin"));
        assert_eq!(admin.test_robot("ghost", "p1", "admin").await, Err(RuleAdminError::NotFound));
    }

    #[tokio::test]
    async fn rule_crud_roundtrip() {
        let (_s, _tx, admin) = admin();
        let draft = RuleDraft {
            project_id: "p1".into(),
            event_type: "BUG_ASSIGNED".into(),
            channels: vec![Channel::InApp, Channel::Robot],
            robot_ids: vec!["r1".into()],
            template: "${title}".into(),
            enabled: true,
        };
        let r = admin.create_rule(draft.clone()).await.expect("create");
        assert_eq!(r.event_type, "BUG_ASSIGNED");
        let mut d2 = draft.clone();
        d2.event_type = "*".into();
        let r2 = admin.update_rule(&r.id, d2).await.expect("update");
        assert_eq!(r2.event_type, "*");
        assert_eq!(admin.list_rules("p1").await.expect("list").len(), 1);
        admin.delete_rule(&r.id, "p1").await.expect("delete");
        assert_eq!(admin.delete_rule(&r.id, "p1").await, Err(RuleAdminError::NotFound));
    }

    #[tokio::test]
    async fn invalid_drafts_are_rejected() {
        let (_s, _tx, admin) = admin();
        let mut d = robot_draft("bot");
        d.webhook_url = " ".into();
        assert!(matches!(
            admin.create_robot(d).await,
            Err(RuleAdminError::Invalid(NoticeError::InvalidRobot))
        ));
        let bad = RuleDraft {
            project_id: "p1".into(),
            event_type: "".into(),
            channels: vec![],
            robot_ids: vec![],
            template: String::new(),
            enabled: true,
        };
        assert!(matches!(
            admin.create_rule(bad).await,
            Err(RuleAdminError::Invalid(NoticeError::InvalidRule))
        ));
    }
}

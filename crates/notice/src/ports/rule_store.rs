use async_trait::async_trait;

use crate::domain::{Robot, RobotDraft, Rule, RuleDraft};
use crate::ports::notice_store::RepoError;

/// Persistence for notification robots and routing rules (project-scoped).
#[async_trait]
pub trait NoticeRuleStore: Send + Sync {
    async fn insert_robot(&self, draft: &RobotDraft) -> Result<Robot, RepoError>;

    /// Updates the robot; None when the id doesn't exist in the draft's project.
    async fn update_robot(&self, id: &str, draft: &RobotDraft) -> Result<Option<Robot>, RepoError>;

    /// false when the id doesn't exist in the project.
    async fn delete_robot(&self, id: &str, project_id: &str) -> Result<bool, RepoError>;

    async fn list_robots(&self, project_id: &str) -> Result<Vec<Robot>, RepoError>;

    async fn get_robot(&self, id: &str, project_id: &str) -> Result<Option<Robot>, RepoError>;

    /// Robots matching any of the ids (missing ids are silently dropped).
    async fn robots_by_ids(&self, ids: &[String]) -> Result<Vec<Robot>, RepoError>;

    async fn insert_rule(&self, draft: &RuleDraft) -> Result<Rule, RepoError>;

    /// Updates the rule; None when the id doesn't exist in the draft's project.
    async fn update_rule(&self, id: &str, draft: &RuleDraft) -> Result<Option<Rule>, RepoError>;

    /// false when the id doesn't exist in the project.
    async fn delete_rule(&self, id: &str, project_id: &str) -> Result<bool, RepoError>;

    async fn list_rules(&self, project_id: &str) -> Result<Vec<Rule>, RepoError>;

    /// Enabled rules matching (project, event_type) plus the `*` wildcard.
    async fn rules_for_event(
        &self,
        project_id: &str,
        event_type: &str,
    ) -> Result<Vec<Rule>, RepoError>;
}

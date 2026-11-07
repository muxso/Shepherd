use crate::domain::code_authors::CodeAuthorsQuery;
use crate::domain::{
    CompleteRun, Error, ListQuery, MemberMetricsQuery, NewSource, Record, RequirementOutputQuery,
};
use async_trait::async_trait;
use serde_json::Value;
#[async_trait]
pub trait Store: Send + Sync {
    async fn activity_review_policy(&self, source: &str) -> Result<Value, Error>;
    async fn set_activity_review_policy(
        &self,
        source: &str,
        policy: crate::domain::activity_review::ActivityReviewPolicy,
    ) -> Result<Value, Error>;
    async fn activity_members(&self, source: &str) -> Result<Value, Error>;
    async fn set_activity_members(
        &self,
        source: &str,
        config: crate::domain::team_activity::ActivityMembers,
    ) -> Result<Value, Error>;
    async fn team_activity(
        &self,
        source: &str,
        query: crate::domain::team_activity::ActivityQuery,
        permissions: crate::domain::team_activity::ActivityPermissions,
    ) -> Result<Value, Error>;
    async fn author_identities(&self, source: &str) -> Result<Value, Error>;
    async fn set_author_identities(
        &self,
        source: &str,
        config: crate::domain::author_identities::AuthorIdentities,
    ) -> Result<Value, Error>;
    async fn sources(&self) -> Result<Value, Error>;
    async fn create_source(&self, source: NewSource) -> Result<Value, Error>;
    async fn start(&self, source: &str) -> Result<Value, Error>;
    async fn stage(&self, run: &str, records: Vec<Record>) -> Result<Value, Error>;
    async fn complete(&self, run: &str, body: CompleteRun) -> Result<Value, Error>;
    async fn fail(&self, run: &str, error: &str) -> Result<Value, Error>;
    async fn code_authors(
        &self,
        source: &str,
        query: CodeAuthorsQuery,
        can_requirements: bool,
    ) -> Result<Value, Error>;
    async fn dashboard(&self, source: &str, days: i64) -> Result<Value, Error>;
    async fn requirement_output(&self, query: RequirementOutputQuery) -> Result<Value, Error>;
    async fn member_metrics(&self, source: &str, query: MemberMetricsQuery)
        -> Result<Value, Error>;
    async fn records(&self, source: &str, query: ListQuery) -> Result<Value, Error>;
}

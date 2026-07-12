use async_trait::async_trait;

use crate::domain::{ImportSchedule, NewImportSchedule};
use crate::ports::RepoError;

#[async_trait]
pub trait ImportScheduleStore: Send + Sync {
    async fn insert(&self, s: &NewImportSchedule) -> Result<ImportSchedule, RepoError>;
    async fn list_by_project(&self, project_id: &str) -> Result<Vec<ImportSchedule>, RepoError>;
    async fn list_enabled(&self) -> Result<Vec<ImportSchedule>, RepoError>;
    async fn get(&self, id: &str) -> Result<Option<ImportSchedule>, RepoError>;
    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), RepoError>;
    async fn delete(&self, id: &str) -> Result<(), RepoError>;
    /// An empty `operator` marks an automatic cron run.
    async fn record_run(&self, id: &str, result: &str, operator: &str) -> Result<(), RepoError>;
}

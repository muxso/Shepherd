//! 定时导入计划注册表端口。

use async_trait::async_trait;

use crate::domain::{ImportSchedule, NewImportSchedule};
use crate::ports::RepoError;

#[async_trait]
pub trait ImportScheduleStore: Send + Sync {
    /// 登记一条定时导入计划。
    async fn insert(&self, s: &NewImportSchedule) -> Result<ImportSchedule, RepoError>;
    /// 列出某项目的全部计划(排除软删除,时间倒序)。
    async fn list_by_project(&self, project_id: &str) -> Result<Vec<ImportSchedule>, RepoError>;
    /// 列出全部启用计划(供调度器启动时加载)。
    async fn list_enabled(&self) -> Result<Vec<ImportSchedule>, RepoError>;
    /// 按 id 读一条计划(排除软删除)。
    async fn get(&self, id: &str) -> Result<Option<ImportSchedule>, RepoError>;
    /// 启用/停用。
    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), RepoError>;
    /// 软删除。
    async fn delete(&self, id: &str) -> Result<(), RepoError>;
    /// 记录一次运行的时间、结果摘要与操作人(`operator` 为空串表示 cron 自动运行)。
    async fn record_run(&self, id: &str, result: &str, operator: &str) -> Result<(), RepoError>;
}

//! 交付仓储端口。

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{
    AttemptStatus, DeliveryAttempt, ExecutionEvent, ExecutorKind, NewExecutionEvent,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// 任务中心列表过滤/分页条件(全系统范围的交付尝试聚合视图)。
#[derive(Debug, Clone, Default)]
pub struct TaskListFilter {
    /// 仅某执行状态(None 不限)。
    pub status: Option<AttemptStatus>,
    /// 仅某执行者(None 不限)。
    pub executor: Option<ExecutorKind>,
    /// 仅未终态(实时任务页:DISPATCHED/RUNNING)。
    pub active_only: bool,
    /// 模糊匹配任务标题 / taskId / decompositionId(None 不限)。
    pub query: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

/// 任务中心一行:交付尝试 + 关联任务基本信息(标题/描述/所属模块)+ 创建时间 + 事件数(读模型)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRow {
    pub attempt: DeliveryAttempt,
    pub title: Option<String>,
    /// 关联任务描述(ms_task.description;无关联任务则 None)。
    pub description: Option<String>,
    /// 所属模块:任务归属需求的标题(经拆分图→需求关联;无则 None)。
    pub module: Option<String>,
    pub created_at: i64,
    pub event_count: i64,
}

/// 任务中心分页结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPage {
    pub items: Vec<TaskRow>,
    pub total: i64,
}

#[async_trait]
pub trait DeliveryRepository: Send + Sync {
    /// 新建一次交付尝试(分配 id,初始 Dispatched)。
    async fn create(
        &self,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
    ) -> Result<DeliveryAttempt, RepoError>;

    async fn get(&self, id: &str) -> Result<Option<DeliveryAttempt>, RepoError>;

    /// 某任务的全部交付尝试(按创建顺序)。
    async fn list_by_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<DeliveryAttempt>, RepoError>;

    async fn save(&self, attempt: &DeliveryAttempt) -> Result<(), RepoError>;

    /// 追加一条执行事件(分配单调 seq)。
    async fn append_event(
        &self,
        attempt_id: &str,
        event: &NewExecutionEvent,
    ) -> Result<ExecutionEvent, RepoError>;

    /// 某尝试的执行事件(按 seq 升序)。
    async fn list_events(&self, attempt_id: &str) -> Result<Vec<ExecutionEvent>, RepoError>;

    /// 任务中心:跨全系统的交付尝试分页列表(按创建时间倒序),附任务标题/事件数。
    async fn list_tasks(&self, filter: &TaskListFilter) -> Result<TaskPage, RepoError>;

    /// 删除一次交付尝试(其执行事件随外键级联删除)。返回是否删到行。
    async fn delete(&self, id: &str) -> Result<bool, RepoError>;
}

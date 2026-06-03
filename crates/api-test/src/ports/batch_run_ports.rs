//! 批量运行所需的端口:解析项目默认池 + 判断池可用性 + 派发执行。

use async_trait::async_trait;

use crate::domain::BatchRunMode;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("backend error: {0}")]
    Backend(String),
}

/// 资源池信息来源。
#[async_trait]
pub trait ResourcePoolPort: Send + Sync {
    /// 项目配置的默认资源池(可能为空 —— 正是 quirk 的诱因)。
    async fn default_pool_id(&self, project_id: &str) -> Result<Option<String>, PortError>;

    /// 该池是否可用(存在 + 启用 + 有在线节点)。
    async fn is_pool_available(&self, pool_id: &str) -> Result<bool, PortError>;
}

/// 派发到执行器的规格(已解析出有效池)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchSpec {
    pub case_ids: Vec<String>,
    pub pool_id: String,
    pub mode: BatchRunMode,
}

/// 派发结果:报告 id + 派发后即刻的报告状态。
/// 同步 runner 跑完即 SUCCESS/ERROR;异步执行器(JMeter)接受后为 RUNNING。
/// 携带 status 让上游(如场景执行记录)能在派发后立即落到真实状态,而非永远 PENDING。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchReport {
    pub report_id: String,
    pub status: String,
}

/// 批量执行器(对接 JMeter/K8s 的边界;此处抽象出"派发并返回报告 id + 状态")。
#[async_trait]
pub trait BatchExecutorPort: Send + Sync {
    async fn dispatch(&self, spec: &DispatchSpec) -> Result<DispatchReport, PortError>;
}

/// 一个待下发给执行节点的运行任务(报告已落库,带 report_id)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTask {
    pub report_id: String,
    pub pool_id: String,
    pub mode: BatchRunMode,
    pub case_ids: Vec<String>,
}

/// 下发结果:据此决定报告状态。
/// - `Accepted`:已交给远端执行器**异步**运行(如 JMeter 节点)→ 报告置 RUNNING。
/// - `Completed`:执行器**就地同步**跑完(如原生 Rust runner)→ 报告置该最终状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Accepted,
    Completed { status: String },
}

/// 任务下发出站端口:把运行任务交给某种执行器。
/// 实现可为异步 HTTP 下发 JMeter 节点(`Accepted`),也可为就地原生 runner(`Completed`)。
#[async_trait]
pub trait TaskDispatcher: Send + Sync {
    async fn dispatch_task(&self, task: &RunTask) -> Result<DispatchOutcome, PortError>;
}

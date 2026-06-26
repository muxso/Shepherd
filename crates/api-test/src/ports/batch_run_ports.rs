use async_trait::async_trait;

use crate::domain::{BatchRunMode, ResolvedEnv};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait ResourcePoolPort: Send + Sync {
    async fn default_pool_id(&self, project_id: &str) -> Result<Option<String>, PortError>;

    async fn is_pool_available(&self, pool_id: &str) -> Result<bool, PortError>;
}

#[async_trait]
pub trait EnvironmentPort: Send + Sync {
    async fn resolve(&self, environment_id: &str) -> Result<Option<ResolvedEnv>, PortError>;
}

#[async_trait]
pub trait EnvVarWriter: Send + Sync {
    async fn set_vars(&self, environment_id: &str, vars: &[(String, String)]) -> Result<(), PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchSpec {
    pub case_ids: Vec<String>,
    pub pool_id: String,
    pub mode: BatchRunMode,
    pub env: ResolvedEnv,
    pub environment_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchReport {
    pub report_id: String,
    pub status: String,
}

#[async_trait]
pub trait BatchExecutorPort: Send + Sync {
    async fn dispatch(&self, spec: &DispatchSpec) -> Result<DispatchReport, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTask {
    pub report_id: String,
    pub pool_id: String,
    pub mode: BatchRunMode,
    pub case_ids: Vec<String>,
    pub env: ResolvedEnv,
    pub environment_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    Accepted,
    Completed { status: String },
}

#[async_trait]
pub trait TaskDispatcher: Send + Sync {
    async fn dispatch_task(&self, task: &RunTask) -> Result<DispatchOutcome, PortError>;
}

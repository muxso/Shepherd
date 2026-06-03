//! 测试替身:可配置资源池(Fake)+ 记录派发的执行器(Spy)。
//!
//! 这正是六边形架构的回报:批量运行的核心规则无需真起 JMeter/K8s/资源池服务,
//! 用可信替身就能把"池解析优先级、不可用拒绝、不派发"等断言钉死。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::ports::{
    BatchExecutorPort, DispatchOutcome, DispatchReport, DispatchSpec, PortError, ResourcePoolPort,
    RunTask, TaskDispatcher,
};

/// 可配置的资源池替身:项目默认池映射 + 可用池集合。
#[derive(Clone, Default)]
pub struct FakeResourcePool {
    defaults: HashMap<String, String>,
    available: HashSet<String>,
}

impl FakeResourcePool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_default(mut self, project_id: &str, pool_id: &str) -> Self {
        self.defaults.insert(project_id.to_string(), pool_id.to_string());
        self
    }

    pub fn with_available(mut self, pool_id: &str) -> Self {
        self.available.insert(pool_id.to_string());
        self
    }
}

#[async_trait]
impl ResourcePoolPort for FakeResourcePool {
    async fn default_pool_id(&self, project_id: &str) -> Result<Option<String>, PortError> {
        Ok(self.defaults.get(project_id).cloned())
    }

    async fn is_pool_available(&self, pool_id: &str) -> Result<bool, PortError> {
        Ok(self.available.contains(pool_id))
    }
}

/// 记录每次派发的执行器替身,便于断言"用哪个池、派发了几条、派发了几次"。
#[derive(Clone, Default)]
pub struct SpyExecutor {
    dispatches: Arc<Mutex<Vec<DispatchSpec>>>,
}

impl SpyExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn dispatch_count(&self) -> usize {
        self.dispatches.lock().expect("lock").len()
    }

    pub fn last_pool(&self) -> Option<String> {
        self.dispatches.lock().expect("lock").last().map(|d| d.pool_id.clone())
    }

    pub fn last_case_count(&self) -> Option<usize> {
        self.dispatches.lock().expect("lock").last().map(|d| d.case_ids.len())
    }
}

#[async_trait]
impl BatchExecutorPort for SpyExecutor {
    async fn dispatch(&self, spec: &DispatchSpec) -> Result<DispatchReport, PortError> {
        let mut d = self.dispatches.lock().expect("lock");
        d.push(spec.clone());
        Ok(DispatchReport { report_id: format!("report-{}", d.len()), status: "SUCCESS".to_string() })
    }
}

/// 记录下发任务的 `TaskDispatcher` 替身;可配置为失败,验证报告状态置 DISPATCH_FAILED。
#[derive(Clone, Default)]
pub struct SpyDispatcher {
    tasks: Arc<Mutex<Vec<RunTask>>>,
    fail: bool,
}

impl SpyDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// 让下发失败(模拟执行节点不可达)。
    pub fn failing() -> Self {
        Self { fail: true, ..Self::default() }
    }

    pub fn count(&self) -> usize {
        self.tasks.lock().expect("lock").len()
    }

    pub fn last(&self) -> Option<RunTask> {
        self.tasks.lock().expect("lock").last().cloned()
    }
}

#[async_trait]
impl TaskDispatcher for SpyDispatcher {
    async fn dispatch_task(&self, task: &RunTask) -> Result<DispatchOutcome, PortError> {
        self.tasks.lock().expect("lock").push(task.clone());
        if self.fail {
            return Err(PortError::Backend("executor node unreachable".into()));
        }
        Ok(DispatchOutcome::Accepted)
    }
}

/// 什么都不做的下发器(本地起服务、无执行节点时占位):接受并异步化(RUNNING)。
#[derive(Clone, Default)]
pub struct NoopDispatcher;

#[async_trait]
impl TaskDispatcher for NoopDispatcher {
    async fn dispatch_task(&self, _task: &RunTask) -> Result<DispatchOutcome, PortError> {
        Ok(DispatchOutcome::Accepted)
    }
}

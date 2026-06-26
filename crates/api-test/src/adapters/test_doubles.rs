use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::ResolvedEnv;
use crate::ports::{
    BatchExecutorPort, DispatchOutcome, DispatchReport, DispatchSpec, EnvironmentPort, PortError,
    ResourcePoolPort, RunTask, TaskDispatcher,
};

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

    pub fn last_env(&self) -> Option<ResolvedEnv> {
        self.dispatches.lock().expect("lock").last().map(|d| d.env.clone())
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

#[derive(Clone, Default)]
pub struct SpyDispatcher {
    tasks: Arc<Mutex<Vec<RunTask>>>,
    fail: bool,
}

impl SpyDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

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

#[derive(Clone, Default)]
pub struct FakeEnvironment {
    envs: HashMap<String, ResolvedEnv>,
}

impl FakeEnvironment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, id: &str, env: ResolvedEnv) -> Self {
        self.envs.insert(id.to_string(), env);
        self
    }
}

#[async_trait]
impl EnvironmentPort for FakeEnvironment {
    async fn resolve(&self, environment_id: &str) -> Result<Option<ResolvedEnv>, PortError> {
        Ok(self.envs.get(environment_id).cloned())
    }
}

#[derive(Clone, Default)]
pub struct NoopDispatcher;

#[async_trait]
impl TaskDispatcher for NoopDispatcher {
    async fn dispatch_task(&self, _task: &RunTask) -> Result<DispatchOutcome, PortError> {
        Ok(DispatchOutcome::Accepted)
    }
}

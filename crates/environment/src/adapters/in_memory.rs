//! 内存版环境仓储。Mutex 套 HashMap,id 自增,测试用。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{Environment, NewEnvironment};
use crate::ports::{EnvironmentRepository, RepoError};

#[derive(Default)]
struct State {
    envs: HashMap<String, Environment>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryEnvironmentRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryEnvironmentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

fn to_env(id: String, e: &NewEnvironment) -> Environment {
    Environment {
        id,
        project_id: e.project_id.clone(),
        name: e.name.clone(),
        base_url: e.base_url.clone(),
        headers: e.headers.clone(),
        variables: e.variables.clone(),
        enabled: e.enabled,
    }
}

#[async_trait]
impl EnvironmentRepository for InMemoryEnvironmentRepository {
    async fn insert(&self, e: &NewEnvironment) -> Result<Environment, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let env = to_env(format!("env-{}", state.seq), e);
        state.envs.insert(env.id.clone(), env.clone());
        Ok(env)
    }

    async fn get(&self, id: &str) -> Result<Option<Environment>, RepoError> {
        Ok(self.state.lock().expect("lock").envs.get(id).cloned())
    }

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<Environment>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<Environment> = state
            .envs
            .values()
            .filter(|e| e.project_id == project_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn update(&self, id: &str, e: &NewEnvironment) -> Result<Option<Environment>, RepoError> {
        let mut state = self.state.lock().expect("lock");
        if !state.envs.contains_key(id) {
            return Ok(None);
        }
        // project_id 不可变:沿用既有行的 project_id。
        let project_id = state.envs[id].project_id.clone();
        let mut env = to_env(id.to_string(), e);
        env.project_id = project_id;
        state.envs.insert(id.to_string(), env.clone());
        Ok(Some(env))
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        Ok(self.state.lock().expect("lock").envs.remove(id).is_some())
    }
}

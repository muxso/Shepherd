//! 内存版缺陷仓储。可按项目配置状态流图(测试用 `with_default_flow` 注入典型缺陷流)。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{Bug, NewBug, StatusFlowGraph};
use crate::ports::{BugRepository, RepoError};

#[derive(Default)]
struct State {
    flows: HashMap<String, StatusFlowGraph>, // project_id -> 状态流图
    bugs: HashMap<String, Bug>,              // bug_id -> bug
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryBugRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryBugRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// 为某项目配置状态流图。
    pub fn set_flow(&self, project_id: &str, flow: StatusFlowGraph) {
        self.state.lock().expect("lock").flows.insert(project_id.to_string(), flow);
    }

    /// 便捷构造:为项目装上默认缺陷流(种子,复用领域 `default_bug_flow`)。
    pub fn with_default_flow(project_id: &str) -> Self {
        let repo = Self::new();
        repo.set_flow(project_id, StatusFlowGraph::default_bug_flow());
        repo
    }

    /// 测试辅助:读缺陷当前状态。
    pub fn status_of(&self, bug_id: &str) -> Option<String> {
        self.state.lock().expect("lock").bugs.get(bug_id).map(|b| b.status.clone())
    }
}

#[async_trait]
impl BugRepository for InMemoryBugRepository {
    async fn status_flow(&self, project_id: &str) -> Result<StatusFlowGraph, RepoError> {
        Ok(self.state.lock().expect("lock").flows.get(project_id).cloned().unwrap_or_default())
    }

    async fn insert(&self, new_bug: &NewBug, initial_status: &str) -> Result<Bug, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let bug = Bug {
            id: format!("bug-{}", state.seq),
            project_id: new_bug.project_id.clone(),
            title: new_bug.title.clone(),
            status: initial_status.to_string(),
            deleted: false,
        };
        state.bugs.insert(bug.id.clone(), bug.clone());
        Ok(bug)
    }

    async fn get(&self, id: &str) -> Result<Option<Bug>, RepoError> {
        Ok(self.state.lock().expect("lock").bugs.get(id).cloned())
    }

    async fn set_status(&self, id: &str, status: &str) -> Result<(), RepoError> {
        if let Some(b) = self.state.lock().expect("lock").bugs.get_mut(id) {
            b.status = status.to_string();
        }
        Ok(())
    }
}

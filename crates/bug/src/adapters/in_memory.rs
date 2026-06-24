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
    order: Vec<String>,                      // 插入顺序的 bug_id(列表倒序展示用)
    followers: HashMap<String, Vec<String>>, // bug_id -> 关注人 user_id(按关注先后)
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
        let seq = state.seq;
        let bug = Bug {
            id: format!("bug-{seq}"),
            project_id: new_bug.project_id.clone(),
            title: new_bug.title.clone(),
            status: initial_status.to_string(),
            deleted: false,
            created_at: seq as i64, // 单调递增,充当列表倒序的排序键
            created_by: new_bug.created_by.clone(),
        };
        state.order.push(bug.id.clone());
        state.bugs.insert(bug.id.clone(), bug.clone());
        Ok(bug)
    }

    async fn list(&self, project_id: &str) -> Result<Vec<Bug>, RepoError> {
        let state = self.state.lock().expect("lock");
        // 按插入顺序倒序(新建在前),只看该项目未删除的缺陷。
        Ok(state
            .order
            .iter()
            .rev()
            .filter_map(|id| state.bugs.get(id))
            .filter(|b| b.project_id == project_id && !b.deleted)
            .cloned()
            .collect())
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

    async fn add_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        let followers = state.followers.entry(bug_id.to_string()).or_default();
        if !followers.iter().any(|u| u == user_id) {
            followers.push(user_id.to_string());
        }
        Ok(())
    }

    async fn remove_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        if let Some(followers) = state.followers.get_mut(bug_id) {
            followers.retain(|u| u != user_id);
        }
        Ok(())
    }

    async fn list_followers(&self, bug_id: &str) -> Result<Vec<String>, RepoError> {
        Ok(self.state.lock().expect("lock").followers.get(bug_id).cloned().unwrap_or_default())
    }
}

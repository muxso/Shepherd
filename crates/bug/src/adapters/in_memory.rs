use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{Bug, BugRelation, NewBug, RelationKind, StatusFlowGraph};
use crate::ports::{BugRepository, RepoError};

#[derive(Default)]
struct State {
    flows: HashMap<String, StatusFlowGraph>,
    bugs: HashMap<String, Bug>,
    order: Vec<String>,
    followers: HashMap<String, Vec<String>>,
    relations: HashMap<String, Vec<BugRelation>>,
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

    pub fn set_flow(&self, project_id: &str, flow: StatusFlowGraph) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .flows
            .insert(project_id.to_string(), flow);
    }

    pub fn with_default_flow(project_id: &str) -> Self {
        let repo = Self::new();
        repo.set_flow(project_id, StatusFlowGraph::default_bug_flow());
        repo
    }

    pub fn status_of(&self, bug_id: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bugs
            .get(bug_id)
            .map(|b| b.status.clone())
    }
}

/// Synthetic monotonic timestamp text: sorts like the pg timestamptz text form.
fn tick(seq: u64) -> String {
    format!("1970-01-01 00:00:00.{seq:06}+00")
}

#[async_trait]
impl BugRepository for InMemoryBugRepository {
    async fn status_flow(&self, project_id: &str) -> Result<StatusFlowGraph, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .flows
            .get(project_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn insert(&self, new_bug: &NewBug, initial_status: &str) -> Result<Bug, RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.seq += 1;
        let seq = state.seq;
        let bug = Bug {
            id: format!("bug-{seq}"),
            project_id: new_bug.project_id.clone(),
            title: new_bug.title.clone(),
            status: initial_status.to_string(),
            deleted: false,
            created_at: seq as i64,
            created_by: new_bug.created_by.clone(),
            severity: new_bug.severity.clone(),
            handler: new_bug.handler.clone(),
            updated_by: new_bug.created_by.clone(),
            updated_at: Some(tick(seq)),
            custom_fields: new_bug.custom_fields.clone(),
        };
        state.order.push(bug.id.clone());
        state.bugs.insert(bug.id.clone(), bug.clone());
        Ok(bug)
    }

    async fn list(&self, project_id: &str) -> Result<Vec<Bug>, RepoError> {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bugs
            .get(id)
            .cloned())
    }

    async fn set_status(
        &self,
        id: &str,
        status: &str,
        operator: Option<&str>,
    ) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.seq += 1;
        let seq = state.seq;
        if let Some(b) = state.bugs.get_mut(id) {
            b.status = status.to_string();
            b.updated_by = operator.map(str::to_string);
            b.updated_at = Some(tick(seq));
        }
        Ok(())
    }

    async fn update_meta(
        &self,
        id: &str,
        title: &str,
        severity: Option<&str>,
        handler: Option<&str>,
        operator: Option<&str>,
    ) -> Result<Option<Bug>, RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.seq += 1;
        let seq = state.seq;
        Ok(state.bugs.get_mut(id).map(|b| {
            b.title = title.to_string();
            b.severity = severity.map(str::to_string);
            b.handler = handler.map(str::to_string);
            b.updated_by = operator.map(str::to_string);
            b.updated_at = Some(tick(seq));
            b.clone()
        }))
    }

    async fn set_custom_fields(
        &self,
        id: &str,
        fields: &BTreeMap<String, String>,
        operator: Option<&str>,
    ) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.seq += 1;
        let seq = state.seq;
        if let Some(b) = state.bugs.get_mut(id) {
            b.custom_fields = fields.clone();
            b.updated_by = operator.map(str::to_string);
            b.updated_at = Some(tick(seq));
        }
        Ok(())
    }

    async fn add_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let followers = state.followers.entry(bug_id.to_string()).or_default();
        if !followers.iter().any(|u| u == user_id) {
            followers.push(user_id.to_string());
        }
        Ok(())
    }

    async fn remove_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(followers) = state.followers.get_mut(bug_id) {
            followers.retain(|u| u != user_id);
        }
        Ok(())
    }

    async fn list_followers(&self, bug_id: &str) -> Result<Vec<String>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .followers
            .get(bug_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn add_relation(&self, rel: &BugRelation) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let rels = state.relations.entry(rel.bug_id.clone()).or_default();
        if !rels.iter().any(|r| r.kind == rel.kind && r.target_id == rel.target_id) {
            rels.push(rel.clone());
        }
        Ok(())
    }

    async fn remove_relation(&self, rel: &BugRelation) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(rels) = state.relations.get_mut(&rel.bug_id) {
            rels.retain(|r| !(r.kind == rel.kind && r.target_id == rel.target_id));
        }
        Ok(())
    }

    async fn list_relations(&self, bug_id: &str) -> Result<Vec<BugRelation>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .relations
            .get(bug_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn list_bugs_by_relation(
        &self,
        kind: RelationKind,
        target_id: &str,
    ) -> Result<Vec<Bug>, RepoError> {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // Newest first, matching the pg adapter's ordering.
        Ok(state
            .order
            .iter()
            .rev()
            .filter(|id| {
                state.relations.get(*id).is_some_and(|rels| {
                    rels.iter().any(|r| r.kind == kind && r.target_id == target_id)
                })
            })
            .filter_map(|id| state.bugs.get(id))
            .filter(|b| !b.deleted)
            .cloned()
            .collect())
    }
}

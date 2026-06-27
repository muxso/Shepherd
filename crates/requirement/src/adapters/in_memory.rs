use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewRequirement, Requirement};
use crate::ports::{RepoError, RequirementRepository};

#[derive(Default)]
struct State {
    requirements: Vec<Requirement>,
    seq: u64,
    /// 需求 id → 显式秩(reorder 写入);未排序的回落到插入序。
    order: std::collections::HashMap<String, i64>,
}

#[derive(Clone, Default)]
pub struct InMemoryRequirementRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryRequirementRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn soft_delete(&self, id: &str) {
        let mut st = self.state.lock().expect("lock poisoned");
        if let Some(r) = st.requirements.iter_mut().find(|r| r.id == id) {
            r.soft_delete();
        }
    }
}

#[async_trait]
impl RequirementRepository for InMemoryRequirementRepository {
    async fn find_active_by_title(
        &self,
        project_id: &str,
        title: &str,
    ) -> Result<Option<Requirement>, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock poisoned")
            .requirements
            .iter()
            .find(|r| r.occupies_title() && r.project_id == project_id && r.title == title)
            .cloned())
    }

    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError> {
        let mut st = self.state.lock().expect("lock poisoned");
        st.seq += 1;
        let req = Requirement::create(&format!("requirement-{}", st.seq), new);
        st.requirements.push(req.clone());
        Ok(req)
    }

    async fn get(&self, id: &str) -> Result<Option<Requirement>, RepoError> {
        Ok(self.state.lock().expect("lock").requirements.iter().find(|r| r.id == id).cloned())
    }

    async fn count_active(&self, project_id: &str) -> Result<u64, RepoError> {
        Ok(self
            .state
            .lock()
            .expect("lock")
            .requirements
            .iter()
            .filter(|r| r.occupies_title() && r.project_id == project_id)
            .count() as u64)
    }

    async fn list_active(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Requirement>, RepoError> {
        let st = self.state.lock().expect("lock");
        // (显式秩, 插入序) 排序:未排序的秩为 0 → 按插入序;与历史行为一致。
        let mut items: Vec<(usize, &Requirement)> = st
            .requirements
            .iter()
            .enumerate()
            .filter(|(_, r)| r.occupies_title() && r.project_id == project_id)
            .collect();
        items.sort_by_key(|(idx, r)| (st.order.get(&r.id).copied().unwrap_or(0), *idx));
        Ok(items
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .map(|(_, r)| r.clone())
            .collect())
    }

    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(slot) = st.requirements.iter_mut().find(|r| r.id == requirement.id) {
            *slot = requirement.clone();
        }
        Ok(())
    }

    async fn set_order(&self, project_id: &str, ordered_ids: &[String]) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        for (i, id) in ordered_ids.iter().enumerate() {
            if st.requirements.iter().any(|r| r.id == *id && r.project_id == project_id) {
                st.order.insert(id.clone(), i as i64 + 1);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_get_save_roundtrip() {
        let repo = InMemoryRequirementRepository::new();
        let nu = NewRequirement::new("p1", "登录", "d", &["c1".to_string()]).expect("v");
        let mut r = repo.insert(&nu).await.expect("insert");
        assert_eq!(r.id, "requirement-1");

        r.revise("v2", vec![]).expect("revise");
        repo.save(&r).await.expect("save");
        let got = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(got.latest_version(), 2);
    }

    #[tokio::test]
    async fn list_defaults_to_insertion_order_then_honors_set_order() {
        let repo = InMemoryRequirementRepository::new();
        let mut ids = Vec::new();
        for t in ["A", "B", "C"] {
            let nu = NewRequirement::new("p1", t, "d", &[]).expect("v");
            ids.push(repo.insert(&nu).await.expect("insert").id);
        }
        // 默认:插入序 A,B,C。
        let titles = |rs: &[Requirement]| rs.iter().map(|r| r.title.clone()).collect::<Vec<_>>();
        let listed = repo.list_active("p1", 0, 10).await.expect("list");
        assert_eq!(titles(&listed), ["A", "B", "C"]);

        // 排序:C,A,B。
        repo.set_order("p1", &[ids[2].clone(), ids[0].clone(), ids[1].clone()]).await.expect("order");
        let listed = repo.list_active("p1", 0, 10).await.expect("list");
        assert_eq!(titles(&listed), ["C", "A", "B"]);
    }

    #[tokio::test]
    async fn set_order_ignores_other_projects_ids() {
        let repo = InMemoryRequirementRepository::new();
        let a = repo.insert(&NewRequirement::new("p1", "A", "d", &[]).expect("v")).await.expect("i");
        let _other =
            repo.insert(&NewRequirement::new("p2", "X", "d", &[]).expect("v")).await.expect("i");
        // 传入跨项目 id 不应影响 p1 的排序写入(仅本项目存在的 id 生效)。
        repo.set_order("p1", &["nope".to_string(), a.id.clone()]).await.expect("order");
        let listed = repo.list_active("p1", 0, 10).await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "A");
    }

    #[tokio::test]
    async fn find_active_ignores_soft_deleted() {
        let repo = InMemoryRequirementRepository::new();
        let nu = NewRequirement::new("p1", "登录", "d", &[]).expect("v");
        let r = repo.insert(&nu).await.expect("insert");
        assert!(repo.find_active_by_title("p1", "登录").await.expect("q").is_some());
        repo.soft_delete(&r.id);
        assert!(repo.find_active_by_title("p1", "登录").await.expect("q").is_none());
    }
}

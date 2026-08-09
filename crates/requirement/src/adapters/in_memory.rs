use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{
    fill_stages, ChangeEntry, NewChange, NewRequirement, Requirement, StageRow, StatusCounts,
};
use crate::ports::{RepoError, RequirementRepository};

#[derive(Default)]
struct State {
    requirements: Vec<Requirement>,
    seq: u64,
    /// Requirement id → explicit rank (written by reorder); unranked ones fall back to insertion order.
    order: std::collections::HashMap<String, i64>,
    /// Change log, stored in append order; read in reverse (newest first).
    changes: std::collections::HashMap<String, Vec<ChangeEntry>>,
    /// Stage rows (sparse, only upserted ones); read side fills via `fill_stages`, mirroring the separate table on the pg side.
    stages: std::collections::HashMap<String, Vec<StageRow>>,
    /// Logical clock: +1 per write, keeping created/updated/changed timestamps monotonic and testable.
    clock: i64,
}

impl State {
    fn tick(&mut self) -> i64 {
        self.clock += 1;
        self.clock
    }

    /// Read side fills `stages` from the stage table (missing rows default to PENDING), matching the pg behavior.
    fn with_stages(&self, r: &Requirement) -> Requirement {
        let mut r = r.clone();
        r.stages = fill_stages(self.stages.get(&r.id).map(Vec::as_slice).unwrap_or(&[]));
        r
    }
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
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st
            .requirements
            .iter()
            .find(|r| r.occupies_title() && r.project_id == project_id && r.title == title)
            .map(|r| st.with_stages(r)))
    }

    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        st.seq += 1;
        let id = format!("requirement-{}", st.seq);
        let now = st.tick();
        let mut req = Requirement::create(&id, new);
        req.created_at_ms = now;
        req.updated_at_ms = now;
        st.requirements.push(req.clone());
        Ok(req)
    }

    async fn get(&self, id: &str) -> Result<Option<Requirement>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.requirements.iter().find(|r| r.id == id).map(|r| st.with_stages(r)))
    }

    async fn count_active(&self, project_id: &str) -> Result<u64, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // Sort by (explicit rank, insertion index): unranked rank is 0 → insertion order; matches historical behavior.
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
            .map(|(_, r)| st.with_stages(r))
            .collect())
    }

    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = st.tick();
        if let Some(slot) = st.requirements.iter_mut().find(|r| r.id == requirement.id) {
            *slot = requirement.clone();
            // Match pg's `updated_at = now()`: stamp the update time on every save.
            slot.updated_at_ms = now;
        }
        Ok(())
    }

    async fn set_order(&self, project_id: &str, ordered_ids: &[String]) -> Result<(), RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for (i, id) in ordered_ids.iter().enumerate() {
            if st.requirements.iter().any(|r| r.id == *id && r.project_id == project_id) {
                st.order.insert(id.clone(), i as i64 + 1);
            }
        }
        Ok(())
    }

    async fn status_counts(&self, project_id: &str) -> Result<StatusCounts, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut counts = StatusCounts::default();
        for r in st.requirements.iter().filter(|r| r.occupies_title() && r.project_id == project_id)
        {
            counts.add(r.status);
        }
        Ok(counts)
    }

    async fn children(&self, parent_id: &str) -> Result<Vec<Requirement>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st
            .requirements
            .iter()
            .filter(|r| r.occupies_title() && r.parent_id.as_deref() == Some(parent_id))
            .map(|r| st.with_stages(r))
            .collect())
    }

    async fn upsert_stage(&self, requirement_id: &str, row: &StageRow) -> Result<(), RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let rows = st.stages.entry(requirement_id.to_string()).or_default();
        match rows.iter_mut().find(|r| r.stage == row.stage) {
            Some(slot) => *slot = row.clone(),
            None => rows.push(row.clone()),
        }
        Ok(())
    }

    async fn stages(&self, requirement_id: &str) -> Result<Vec<StageRow>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(fill_stages(st.stages.get(requirement_id).map(Vec::as_slice).unwrap_or(&[])))
    }

    async fn append_change(
        &self,
        requirement_id: &str,
        changes: &[NewChange],
    ) -> Result<(), RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for c in changes {
            let now = st.tick();
            st.changes.entry(requirement_id.to_string()).or_default().push(ChangeEntry {
                changed_at_ms: now,
                changed_by: c.changed_by.clone(),
                field: c.field.clone(),
                old_value: c.old_value.clone(),
                new_value: c.new_value.clone(),
            });
        }
        Ok(())
    }

    async fn list_changes(
        &self,
        requirement_id: &str,
        limit: u32,
    ) -> Result<Vec<ChangeEntry>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .changes
            .get(requirement_id)
            .map(|v| v.iter().rev().take(limit as usize).cloned().collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_get_save_roundtrip() {
        let repo = InMemoryRequirementRepository::new();
        let nu = NewRequirement::new("p1", "login", "d", &["c1".to_string()]).expect("v");
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
        // Default: insertion order A,B,C.
        let titles = |rs: &[Requirement]| rs.iter().map(|r| r.title.clone()).collect::<Vec<_>>();
        let listed = repo.list_active("p1", 0, 10).await.expect("list");
        assert_eq!(titles(&listed), ["A", "B", "C"]);

        // Reordered: C,A,B.
        repo.set_order("p1", &[ids[2].clone(), ids[0].clone(), ids[1].clone()])
            .await
            .expect("order");
        let listed = repo.list_active("p1", 0, 10).await.expect("list");
        assert_eq!(titles(&listed), ["C", "A", "B"]);
    }

    #[tokio::test]
    async fn set_order_ignores_other_projects_ids() {
        let repo = InMemoryRequirementRepository::new();
        let a =
            repo.insert(&NewRequirement::new("p1", "A", "d", &[]).expect("v")).await.expect("i");
        let _other =
            repo.insert(&NewRequirement::new("p2", "X", "d", &[]).expect("v")).await.expect("i");
        // Cross-project ids must not affect p1's ordering (only ids existing in this project take effect).
        repo.set_order("p1", &["nope".to_string(), a.id.clone()]).await.expect("order");
        let listed = repo.list_active("p1", 0, 10).await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "A");
    }

    #[tokio::test]
    async fn status_counts_tallies_active_by_status() {
        let repo = InMemoryRequirementRepository::new();
        // Two DRAFT + one soft-deleted (not counted) + one in another project (not counted).
        let a =
            repo.insert(&NewRequirement::new("p1", "A", "d", &[]).expect("v")).await.expect("i");
        repo.insert(&NewRequirement::new("p1", "B", "d", &[]).expect("v")).await.expect("i");
        let c =
            repo.insert(&NewRequirement::new("p1", "C", "d", &[]).expect("v")).await.expect("i");
        repo.insert(&NewRequirement::new("p2", "X", "d", &[]).expect("v")).await.expect("i");
        repo.soft_delete(&c.id);
        // Promote A to BASELINED.
        let mut ra = repo.get(&a.id).await.expect("g").expect("s");
        ra.set_baseline(1).expect("baseline");
        repo.save(&ra).await.expect("save");

        let counts = repo.status_counts("p1").await.expect("counts");
        assert_eq!(counts.draft, 1); // B
        assert_eq!(counts.baselined, 1); // A
        assert_eq!(counts.delivered, 0);
        assert_eq!(counts.archived, 0);
        assert_eq!(counts.total(), 2); // C soft-deleted, p2 in another project
    }

    #[tokio::test]
    async fn stage_upsert_read_and_get_fill_roundtrip() {
        use crate::domain::{Stage, StageRow, StageStatus};
        let repo = InMemoryRequirementRepository::new();
        let r = repo
            .insert(&NewRequirement::new("p1", "login", "d", &[]).expect("v"))
            .await
            .expect("insert");

        // Before any upsert: 7 PENDING default rows.
        let rows = repo.stages(&r.id).await.expect("stages");
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().all(|s| s.status == StageStatus::Pending));

        // After upserting one row: it is updated, others stay defaulted; a repeat upsert overwrites the same row.
        let mut dev = StageRow::pending(Stage::Dev);
        dev.planned_end = Some("2026-12-31".to_string());
        dev.set_status(StageStatus::InProgress, 100);
        repo.upsert_stage(&r.id, &dev).await.expect("upsert");
        dev.set_status(StageStatus::Done, 200);
        repo.upsert_stage(&r.id, &dev).await.expect("upsert");

        let rows = repo.stages(&r.id).await.expect("stages");
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[3], dev);
        // get returns the requirement with filled stages.
        let got = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(got.stages, rows);
    }

    #[tokio::test]
    async fn find_active_ignores_soft_deleted() {
        let repo = InMemoryRequirementRepository::new();
        let nu = NewRequirement::new("p1", "login", "d", &[]).expect("v");
        let r = repo.insert(&nu).await.expect("insert");
        assert!(repo.find_active_by_title("p1", "login").await.expect("q").is_some());
        repo.soft_delete(&r.id);
        assert!(repo.find_active_by_title("p1", "login").await.expect("q").is_none());
    }
}

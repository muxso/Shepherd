use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{DeliveryAttempt, ExecutionEvent, ExecutorKind, NewExecutionEvent};
use crate::ports::{DeliveryRepository, RepoError, TaskListFilter, TaskPage, TaskRow};

#[derive(Default)]
struct State {
    attempts: Vec<DeliveryAttempt>,
    seq: u64,
    event_seq: i64,
    events: Vec<(String, ExecutionEvent)>,
    created_at: Vec<(String, i64)>,
}

#[derive(Clone, Default)]
pub struct InMemoryDeliveryRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryDeliveryRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DeliveryRepository for InMemoryDeliveryRepository {
    async fn create(
        &self,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
        target_runtime: Option<&str>,
    ) -> Result<DeliveryAttempt, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        st.seq += 1;
        let a = DeliveryAttempt::dispatched(
            &format!("attempt-{}", st.seq),
            decomposition_id,
            task_id,
            executor,
            target_runtime,
        );
        let created = st.seq as i64;
        st.created_at.push((a.id.clone(), created));
        st.attempts.push(a.clone());
        Ok(a)
    }

    async fn get(&self, id: &str) -> Result<Option<DeliveryAttempt>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .attempts
            .iter()
            .find(|a| a.id == id)
            .cloned())
    }

    async fn list_by_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<DeliveryAttempt>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .attempts
            .iter()
            .filter(|a| a.decomposition_id == decomposition_id && a.task_id == task_id)
            .cloned()
            .collect())
    }

    async fn save(&self, attempt: &DeliveryAttempt) -> Result<(), RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(slot) = st.attempts.iter_mut().find(|a| a.id == attempt.id) {
            *slot = attempt.clone();
        }
        Ok(())
    }

    async fn append_event(
        &self,
        attempt_id: &str,
        event: &NewExecutionEvent,
    ) -> Result<ExecutionEvent, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        st.event_seq += 1;
        let e = ExecutionEvent {
            seq: st.event_seq,
            kind: event.kind,
            message: event.message.clone(),
            detail: event.detail.clone(),
        };
        st.events.push((attempt_id.to_string(), e.clone()));
        Ok(e)
    }

    async fn list_events(&self, attempt_id: &str) -> Result<Vec<ExecutionEvent>, RepoError> {
        let mut events: Vec<ExecutionEvent> = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .events
            .iter()
            .filter(|(aid, _)| aid == attempt_id)
            .map(|(_, e)| e.clone())
            .collect();
        events.sort_by_key(|e| e.seq);
        Ok(events)
    }

    async fn list_tasks(&self, filter: &TaskListFilter) -> Result<TaskPage, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let created_of = |id: &str| -> i64 {
            st.created_at.iter().find(|(aid, _)| aid == id).map(|(_, c)| *c).unwrap_or(0)
        };
        let event_count_of =
            |id: &str| -> i64 { st.events.iter().filter(|(aid, _)| aid == id).count() as i64 };

        let mut matched: Vec<TaskRow> = st
            .attempts
            .iter()
            .filter(|a| match filter.status {
                Some(s) => a.status == s,
                None => true,
            })
            .filter(|a| match filter.executor {
                Some(e) => a.executor == e,
                None => true,
            })
            .filter(|a| !filter.active_only || !a.status.is_terminal())
            .filter(|a| match &filter.query {
                Some(q) if !q.trim().is_empty() => {
                    let q = q.trim().to_lowercase();
                    a.task_id.to_lowercase().contains(&q)
                        || a.decomposition_id.to_lowercase().contains(&q)
                }
                _ => true,
            })
            .map(|a| TaskRow {
                attempt: a.clone(),
                title: None,
                description: None,
                module: None,
                created_at: created_of(&a.id),
                event_count: event_count_of(&a.id),
            })
            .collect();

        matched.sort_by(|x, y| y.created_at.cmp(&x.created_at));
        let total = matched.len() as i64;
        let items = matched
            .into_iter()
            .skip(filter.offset.max(0) as usize)
            .take(filter.limit.max(0) as usize)
            .collect();
        Ok(TaskPage { items, total })
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = st.attempts.len();
        st.attempts.retain(|a| a.id != id);
        let removed = st.attempts.len() != before;
        if removed {
            st.events.retain(|(aid, _)| aid != id);
            st.created_at.retain(|(aid, _)| aid != id);
        }
        Ok(removed)
    }
}

//! 内存版定时导入计划注册表。Mutex 套 Vec,id 自增确定性生成,测试用。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{ImportSchedule, NewImportSchedule};
use crate::ports::{ImportScheduleStore, RepoError};

#[derive(Default)]
struct State {
    schedules: Vec<ImportSchedule>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryImportScheduleStore {
    state: Arc<Mutex<State>>,
}

impl InMemoryImportScheduleStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ImportScheduleStore for InMemoryImportScheduleStore {
    async fn insert(&self, s: &NewImportSchedule) -> Result<ImportSchedule, RepoError> {
        let mut st = self.state.lock().expect("lock");
        st.seq += 1;
        let sched = ImportSchedule {
            id: format!("import-sched-{}", st.seq),
            project_id: s.project_id.clone(),
            name: s.name.clone(),
            format: s.format.clone(),
            source_url: s.source_url.clone(),
            auth_token: s.auth_token.clone(),
            basic_auth: s.basic_auth,
            module_id: s.module_id.clone(),
            group_by_tag: s.group_by_tag,
            overwrite: s.overwrite,
            sync_module: s.sync_module,
            cron: s.cron.clone(),
            enabled: s.enabled,
            last_run_at: None,
            last_result: String::new(),
            last_run_by: String::new(),
            created_by: s.created_by.clone(),
            created_at: format!("{:020}", st.seq),
        };
        st.schedules.push(sched.clone());
        Ok(sched)
    }

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<ImportSchedule>, RepoError> {
        let st = self.state.lock().expect("lock");
        let mut out: Vec<ImportSchedule> =
            st.schedules.iter().filter(|s| s.project_id == project_id).cloned().collect();
        out.reverse(); // 时间倒序
        Ok(out)
    }

    async fn list_enabled(&self) -> Result<Vec<ImportSchedule>, RepoError> {
        let st = self.state.lock().expect("lock");
        Ok(st.schedules.iter().filter(|s| s.enabled).cloned().collect())
    }

    async fn get(&self, id: &str) -> Result<Option<ImportSchedule>, RepoError> {
        let st = self.state.lock().expect("lock");
        Ok(st.schedules.iter().find(|s| s.id == id).cloned())
    }

    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        if let Some(s) = st.schedules.iter_mut().find(|s| s.id == id) {
            s.enabled = enabled;
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        st.schedules.retain(|s| s.id != id);
        Ok(())
    }

    async fn record_run(&self, id: &str, result: &str, operator: &str) -> Result<(), RepoError> {
        let mut st = self.state.lock().expect("lock");
        let stamp = format!("run-{}", result.len());
        if let Some(s) = st.schedules.iter_mut().find(|s| s.id == id) {
            s.last_run_at = Some(stamp);
            s.last_result = result.to_string();
            s.last_run_by = operator.to_string();
        }
        Ok(())
    }
}

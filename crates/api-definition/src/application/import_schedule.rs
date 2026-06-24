//! 用例:定时导入计划的增删查与启停 + 运行结果回写。
//!
//! 真正「到点拉取 URL 并导入」的动作含 HTTP IO,放在 server 调度器里(复用
//! [`super::ImportApiDefinitionsUseCase`]);本层只管计划注册表的编排,零 IO。

use std::sync::Arc;

use thiserror::Error;

use crate::domain::{ImportSchedule, ImportScheduleError, NewImportSchedule};
use crate::ports::{ImportScheduleStore, RepoError};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CreateImportScheduleError {
    #[error(transparent)]
    Validation(#[from] ImportScheduleError),
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct ImportScheduleUseCase {
    store: Arc<dyn ImportScheduleStore>,
}

impl ImportScheduleUseCase {
    pub fn new(store: Arc<dyn ImportScheduleStore>) -> Self {
        Self { store }
    }

    /// 登记一条计划(构造的 `new` 在此校验)。
    pub async fn create(
        &self,
        new: NewImportSchedule,
    ) -> Result<ImportSchedule, CreateImportScheduleError> {
        let valid = new.validate()?;
        Ok(self.store.insert(&valid).await?)
    }

    pub async fn list_by_project(&self, project_id: &str) -> Result<Vec<ImportSchedule>, RepoError> {
        self.store.list_by_project(project_id).await
    }

    pub async fn list_enabled(&self) -> Result<Vec<ImportSchedule>, RepoError> {
        self.store.list_enabled().await
    }

    pub async fn get(&self, id: &str) -> Result<Option<ImportSchedule>, RepoError> {
        self.store.get(id).await
    }

    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), RepoError> {
        self.store.set_enabled(id, enabled).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), RepoError> {
        self.store.delete(id).await
    }

    /// 回写一次运行(`operator` 为触发者;cron 自动运行传空串)。
    pub async fn record_run(&self, id: &str, result: &str, operator: &str) -> Result<(), RepoError> {
        self.store.record_run(id, result, operator).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryImportScheduleStore;

    fn sample(project: &str) -> NewImportSchedule {
        NewImportSchedule {
            project_id: project.into(),
            name: "sync".into(),
            format: "postman".into(),
            source_url: "https://h/c.json".into(),
            auth_token: String::new(),
            basic_auth: false,
            module_id: None,
            group_by_tag: true,
            overwrite: true,
            sync_module: false,
            cron: "0 0 2 * * *".into(),
            enabled: true,
            created_by: "admin".into(),
        }
    }

    #[tokio::test]
    async fn create_list_disable_delete() {
        let store = Arc::new(InMemoryImportScheduleStore::new());
        let uc = ImportScheduleUseCase::new(store);
        let s = uc.create(sample("p1")).await.expect("create");
        assert_eq!(uc.list_by_project("p1").await.unwrap().len(), 1);
        assert_eq!(uc.list_enabled().await.unwrap().len(), 1);

        uc.set_enabled(&s.id, false).await.expect("disable");
        assert_eq!(uc.list_enabled().await.unwrap().len(), 0); // 停用后不在启用列表
        assert_eq!(uc.list_by_project("p1").await.unwrap().len(), 1); // 但仍在项目列表

        uc.record_run(&s.id, "新增 3 / 覆盖 1 / 跳过 0", "alice").await.expect("record");
        let got = uc.get(&s.id).await.unwrap().unwrap();
        assert_eq!(got.last_result, "新增 3 / 覆盖 1 / 跳过 0");
        assert_eq!(got.last_run_by, "alice"); // 操作人回写

        uc.delete(&s.id).await.expect("delete");
        assert_eq!(uc.list_by_project("p1").await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn create_rejects_invalid() {
        let store = Arc::new(InMemoryImportScheduleStore::new());
        let uc = ImportScheduleUseCase::new(store);
        let mut bad = sample("p1");
        bad.cron = "".into();
        let err = uc.create(bad).await.unwrap_err();
        assert!(matches!(err, CreateImportScheduleError::Validation(ImportScheduleError::EmptyCron)));
    }
}

//! 内存版接口定义仓储。Mutex 套 HashMap,id 由自增计数器确定性生成,测试用。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{
    ApiCase, ApiDefinition, ApiDefinitionChange, ApiModule, ApiMock, NewApiCase, NewApiDefinition,
    NewApiModule, NewApiMock,
};
use crate::ports::{ApiDefinitionRepository, RepoError};

#[derive(Default)]
struct State {
    definitions: HashMap<String, ApiDefinition>, // id -> 接口定义
    cases: HashMap<String, ApiCase>,             // id -> 用例
    case_order: Vec<String>,                     // 用例插入顺序(项目级分页按此)
    mocks: HashMap<String, ApiMock>,             // id -> Mock
    modules: HashMap<String, ApiModule>,         // id -> 模块
    task_cases: Vec<(String, String, String)>,   // (decomposition_id, task_id, case_id)
    changes: Vec<ApiDefinitionChange>,           // 变更历史(追加序)
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryApiDefinitionRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryApiDefinitionRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ApiDefinitionRepository for InMemoryApiDefinitionRepository {
    async fn insert_definition(
        &self,
        d: &NewApiDefinition,
    ) -> Result<ApiDefinition, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let def = ApiDefinition {
            id: format!("apidef-{}", state.seq),
            project_id: d.project_id.clone(),
            name: d.name.clone(),
            protocol: d.protocol,
            method: d.method.clone(),
            path: d.path.clone(),
            status: d.status,
            module_id: None,
            spec: d.spec.clone(),
        };
        state.definitions.insert(def.id.clone(), def.clone());
        Ok(def)
    }

    async fn get_definition(&self, id: &str) -> Result<Option<ApiDefinition>, RepoError> {
        Ok(self.state.lock().expect("lock").definitions.get(id).cloned())
    }

    async fn update_definition_spec(&self, id: &str, spec: &str) -> Result<(), RepoError> {
        if let Some(d) = self.state.lock().expect("lock").definitions.get_mut(id) {
            d.spec = spec.to_string();
        }
        Ok(())
    }

    async fn record_definition_change(
        &self,
        definition_id: &str,
        action: &str,
        detail: &str,
        actor: &str,
    ) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let seq = state.seq;
        state.changes.push(ApiDefinitionChange {
            id: format!("change-{seq}"),
            definition_id: definition_id.to_string(),
            action: action.to_string(),
            detail: detail.to_string(),
            actor: actor.to_string(),
            // 合成单调递增时间串,保证倒序稳定。
            created_at: format!("{seq:020}"),
        });
        Ok(())
    }

    async fn list_definition_changes(
        &self,
        definition_id: &str,
    ) -> Result<Vec<ApiDefinitionChange>, RepoError> {
        let state = self.state.lock().expect("lock");
        // 倒序(最新在前):追加序逆序遍历。
        Ok(state
            .changes
            .iter()
            .rev()
            .filter(|c| c.definition_id == definition_id)
            .cloned()
            .collect())
    }

    async fn list_definitions(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiDefinition>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ApiDefinition> = state
            .definitions
            .values()
            .filter(|d| d.project_id == project_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn insert_case(&self, c: &NewApiCase) -> Result<ApiCase, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let case = ApiCase {
            id: format!("apicase-{}", state.seq),
            api_definition_id: c.api_definition_id.clone(),
            project_id: c.project_id.clone(),
            name: c.name.clone(),
            method: c.method.clone(),
            url: c.url.clone(),
            body: c.body.clone(),
            assertions: c.assertions.clone(),
            processors: c.processors.clone(),
        };
        state.cases.insert(case.id.clone(), case.clone());
        state.case_order.push(case.id.clone());
        Ok(case)
    }

    async fn list_cases(&self, api_definition_id: &str) -> Result<Vec<ApiCase>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ApiCase> = state
            .cases
            .values()
            .filter(|c| c.api_definition_id == api_definition_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn count_cases_by_project(&self, project_id: &str) -> Result<u64, RepoError> {
        let state = self.state.lock().expect("lock");
        Ok(state.cases.values().filter(|c| c.project_id == project_id).count() as u64)
    }

    async fn list_cases_by_project(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<ApiCase>, RepoError> {
        let state = self.state.lock().expect("lock");
        // 按插入顺序遍历,过滤出项目用例,再做 offset/limit 切片。
        let out: Vec<ApiCase> = state
            .case_order
            .iter()
            .filter_map(|id| state.cases.get(id))
            .filter(|c| c.project_id == project_id)
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(out)
    }

    async fn insert_mock(&self, m: &NewApiMock) -> Result<ApiMock, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let mock = ApiMock {
            id: format!("apimock-{}", state.seq),
            api_definition_id: m.api_definition_id.clone(),
            name: m.name.clone(),
            match_rule: m.match_rule.clone(),
            response_status: m.response_status,
            response_body: m.response_body.clone(),
            enabled: m.enabled,
        };
        state.mocks.insert(mock.id.clone(), mock.clone());
        Ok(mock)
    }

    async fn list_mocks(&self, api_definition_id: &str) -> Result<Vec<ApiMock>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ApiMock> = state
            .mocks
            .values()
            .filter(|m| m.api_definition_id == api_definition_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn insert_module(&self, m: &NewApiModule) -> Result<ApiModule, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let module = ApiModule {
            id: format!("apimod-{}", state.seq),
            project_id: m.project_id.clone(),
            parent_id: m.parent_id.clone(),
            name: m.name.clone(),
        };
        state.modules.insert(module.id.clone(), module.clone());
        Ok(module)
    }

    async fn list_modules(&self, project_id: &str) -> Result<Vec<ApiModule>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ApiModule> =
            state.modules.values().filter(|m| m.project_id == project_id).cloned().collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn rename_module(&self, id: &str, name: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        if let Some(m) = state.modules.get_mut(id) {
            m.name = name.to_string();
        }
        Ok(())
    }

    async fn delete_module(&self, id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.modules.remove(id);
        // 其下定义改为未归类
        for d in state.definitions.values_mut() {
            if d.module_id.as_deref() == Some(id) {
                d.module_id = None;
            }
        }
        Ok(())
    }

    async fn set_definition_module(
        &self,
        definition_id: &str,
        module_id: Option<&str>,
    ) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        if let Some(d) = state.definitions.get_mut(definition_id) {
            d.module_id = module_id.map(str::to_string);
        }
        Ok(())
    }

    async fn link_task_case(&self, decomposition_id: &str, task_id: &str, case_id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        let key = (decomposition_id.to_string(), task_id.to_string(), case_id.to_string());
        if !state.task_cases.contains(&key) {
            state.task_cases.push(key);
        }
        Ok(())
    }

    async fn unlink_task_case(&self, decomposition_id: &str, task_id: &str, case_id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.task_cases.retain(|(d, t, c)| !(d == decomposition_id && t == task_id && c == case_id));
        Ok(())
    }

    async fn list_cases_for_task(&self, decomposition_id: &str, task_id: &str) -> Result<Vec<ApiCase>, RepoError> {
        let state = self.state.lock().expect("lock");
        let ids: Vec<&String> = state
            .task_cases
            .iter()
            .filter(|(d, t, _)| d == decomposition_id && t == task_id)
            .map(|(_, _, c)| c)
            .collect();
        Ok(ids.iter().filter_map(|id| state.cases.get(*id)).cloned().collect())
    }
}

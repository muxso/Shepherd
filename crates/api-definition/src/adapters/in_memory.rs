//! 内存版接口定义仓储。Mutex 套 HashMap,id 由自增计数器确定性生成,测试用。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{ApiCase, ApiDefinition, ApiMock, NewApiCase, NewApiDefinition, NewApiMock};
use crate::ports::{ApiDefinitionRepository, RepoError};

#[derive(Default)]
struct State {
    definitions: HashMap<String, ApiDefinition>, // id -> 接口定义
    cases: HashMap<String, ApiCase>,             // id -> 用例
    mocks: HashMap<String, ApiMock>,             // id -> Mock
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
        };
        state.definitions.insert(def.id.clone(), def.clone());
        Ok(def)
    }

    async fn get_definition(&self, id: &str) -> Result<Option<ApiDefinition>, RepoError> {
        Ok(self.state.lock().expect("lock").definitions.get(id).cloned())
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
        };
        state.cases.insert(case.id.clone(), case.clone());
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
}

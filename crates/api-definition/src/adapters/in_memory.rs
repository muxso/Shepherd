//! 内存版接口定义仓储。Mutex 套 HashMap,id 由自增计数器确定性生成,测试用。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{
    ApiCase, ApiDefinition, ApiDefinitionChange, ApiModule, ApiMock, ApiView, NewApiCase,
    NewApiDefinition, NewApiModule, NewApiMock, NewApiView,
};
use crate::ports::{ApiDefinitionRepository, ProjectMockRow, RepoError};

#[derive(Default)]
struct State {
    definitions: HashMap<String, ApiDefinition>, // id -> 接口定义
    cases: HashMap<String, ApiCase>,             // id -> 用例
    case_order: Vec<String>,                     // 用例插入顺序(项目级分页按此)
    mocks: HashMap<String, ApiMock>,             // id -> Mock
    modules: HashMap<String, ApiModule>,         // id -> 模块
    task_cases: Vec<(String, String, String)>,   // (decomposition_id, task_id, case_id)
    changes: Vec<ApiDefinitionChange>,           // 变更历史(追加序)
    views: Vec<ApiView>,                         // 列表视图(保存的筛选/列设置)
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
            // 内存实现合成编号(对齐 pg 序列起点 100001)。
            num: 100000 + state.seq as i64,
            project_id: d.project_id.clone(),
            name: d.name.clone(),
            protocol: d.protocol,
            method: d.method.clone(),
            path: d.path.clone(),
            status: d.status,
            module_id: None,
            spec: d.spec.clone(),
            created_by: d.created_by.clone(),
            // 内存实现无真实时钟,用序号合成单调时间串。
            created_at: format!("{:020}", state.seq),
            updated_at: format!("{:020}", state.seq),
        };
        state.definitions.insert(def.id.clone(), def.clone());
        Ok(def)
    }

    async fn get_definition(&self, id: &str) -> Result<Option<ApiDefinition>, RepoError> {
        Ok(self.state.lock().expect("lock").definitions.get(id).cloned())
    }

    async fn update_definition(
        &self,
        id: &str,
        name: &str,
        protocol: &str,
        method: &str,
        path: &str,
    ) -> Result<Option<ApiDefinition>, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let seq = state.seq;
        let Some(d) = state.definitions.get_mut(id) else {
            return Ok(None);
        };
        d.name = name.to_string();
        if let Some(p) = crate::domain::ApiProtocol::parse(protocol) {
            d.protocol = p;
        }
        d.method = method.to_string();
        d.path = path.to_string();
        // 合成单调递增的更新时间串(与 insert/spec 保持同形态)。
        d.updated_at = format!("{seq:020}");
        Ok(Some(d.clone()))
    }

    async fn update_definition_spec(&self, id: &str, spec: &str) -> Result<(), RepoError> {
        if let Some(d) = self.state.lock().expect("lock").definitions.get_mut(id) {
            d.spec = spec.to_string();
        }
        Ok(())
    }

    async fn update_definition_status(&self, id: &str, status: &str) -> Result<(), RepoError> {
        if let Some(d) = self.state.lock().expect("lock").definitions.get_mut(id) {
            if let Some(s) = crate::domain::ApiStatus::parse(status) {
                d.status = s;
            }
        }
        Ok(())
    }

    async fn delete_definition(&self, id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.definitions.remove(id);
        // 连带移除其用例(含插入顺序)与 Mock。
        let case_ids: Vec<String> = state
            .cases
            .values()
            .filter(|c| c.api_definition_id == id)
            .map(|c| c.id.clone())
            .collect();
        for cid in &case_ids {
            state.cases.remove(cid);
        }
        state.case_order.retain(|cid| !case_ids.contains(cid));
        state.mocks.retain(|_, m| m.api_definition_id != id);
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
            priority: c.priority.clone(),
            status: c.status.clone(),
            tags: c.tags.clone(),
            headers: c.headers.clone(),
            query_params: c.query_params.clone(),
            rest_params: c.rest_params.clone(),
            auth: c.auth.clone(),
        };
        state.cases.insert(case.id.clone(), case.clone());
        state.case_order.push(case.id.clone());
        Ok(case)
    }

    async fn update_case(&self, id: &str, c: &NewApiCase) -> Result<bool, RepoError> {
        let mut state = self.state.lock().expect("lock");
        let Some(existing) = state.cases.get_mut(id) else { return Ok(false) };
        existing.name = c.name.clone();
        existing.method = c.method.clone();
        existing.url = c.url.clone();
        existing.body = c.body.clone();
        existing.assertions = c.assertions.clone();
        existing.processors = c.processors.clone();
        existing.priority = c.priority.clone();
        existing.status = c.status.clone();
        existing.tags = c.tags.clone();
        existing.headers = c.headers.clone();
        existing.query_params = c.query_params.clone();
        existing.rest_params = c.rest_params.clone();
        existing.auth = c.auth.clone();
        Ok(true)
    }

    async fn delete_case(&self, id: &str) -> Result<bool, RepoError> {
        let mut state = self.state.lock().expect("lock");
        let removed = state.cases.remove(id).is_some();
        state.case_order.retain(|cid| cid != id);
        Ok(removed)
    }

    async fn update_mock(&self, mock_id: &str, m: &NewApiMock) -> Result<bool, RepoError> {
        let mut state = self.state.lock().expect("lock");
        let Some(existing) = state.mocks.get_mut(mock_id) else { return Ok(false) };
        // api_definition_id / created_by 不变;其余可变字段整体覆盖。
        existing.name = m.name.clone();
        existing.match_rule = m.match_rule.clone();
        existing.response_status = m.response_status;
        existing.response_body = m.response_body.clone();
        existing.enabled = m.enabled;
        existing.tags = m.tags.clone();
        existing.response_headers = m.response_headers.clone();
        existing.response_delay_ms = m.response_delay_ms;
        existing.follow_definition = m.follow_definition;
        Ok(true)
    }

    async fn delete_mock(&self, mock_id: &str) -> Result<bool, RepoError> {
        Ok(self.state.lock().expect("lock").mocks.remove(mock_id).is_some())
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
            tags: m.tags.clone(),
            response_headers: m.response_headers.clone(),
            response_delay_ms: m.response_delay_ms,
            follow_definition: m.follow_definition,
            created_by: m.created_by.clone(),
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

    async fn list_mocks_by_project(&self, project_id: &str) -> Result<Vec<ProjectMockRow>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ProjectMockRow> = state
            .mocks
            .values()
            .filter_map(|m| {
                let d = state.definitions.get(&m.api_definition_id)?;
                if d.project_id != project_id {
                    return None;
                }
                Some(ProjectMockRow {
                    mock: m.clone(),
                    method: d.method.clone(),
                    path: d.path.clone(),
                    protocol: d.protocol.as_str().to_string(),
                    definition_name: d.name.clone(),
                    operator: String::new(),
                    updated_at: String::new(),
                })
            })
            .collect();
        out.sort_by(|a, b| a.mock.id.cmp(&b.mock.id));
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

    async fn insert_view(&self, v: &NewApiView) -> Result<ApiView, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let view = ApiView {
            id: format!("apiview-{}", state.seq),
            project_id: v.project_id.clone(),
            user_id: v.user_id.clone(),
            name: v.name.clone(),
            config: v.config.clone(),
            shared: v.shared,
            created_at: format!("{:020}", state.seq),
        };
        state.views.push(view.clone());
        Ok(view)
    }

    async fn list_views(&self, project_id: &str, user_id: &str) -> Result<Vec<ApiView>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ApiView> = state
            .views
            .iter()
            .filter(|v| v.project_id == project_id && (v.shared || v.user_id == user_id))
            .cloned()
            .collect();
        out.reverse(); // 创建时间倒序
        Ok(out)
    }

    async fn delete_view(&self, id: &str, user_id: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.views.retain(|v| !(v.id == id && v.user_id == user_id));
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

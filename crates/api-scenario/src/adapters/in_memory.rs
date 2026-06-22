//! 内存版场景仓储。Mutex 守护;id 形如 `scn-{n}` / `step-{n}`。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{
    ApiScenario, ExecutionStatus, NewApiScenario, NewScenarioStep, ScenarioExecution,
    ScenarioReference, ScenarioStatus, ScenarioStep, StepKind,
};
use crate::ports::{ApiScenarioRepository, RepoError};

/// 内部记录:场景元信息 + 其步骤列表。
#[derive(Clone)]
struct Record {
    scenario: ApiScenario,
}

#[derive(Default)]
struct State {
    scenarios: HashMap<String, Record>, // scenario_id -> 记录
    scn_seq: u64,
    step_seq: u64,
    executions: Vec<ScenarioExecution>, // 追加序;created_at 为合成递增串
    exec_seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryApiScenarioRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryApiScenarioRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ApiScenarioRepository for InMemoryApiScenarioRepository {
    async fn insert_scenario(
        &self,
        s: &NewApiScenario,
    ) -> Result<ApiScenario, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.scn_seq += 1;
        let scenario = ApiScenario {
            id: format!("scn-{}", state.scn_seq),
            project_id: s.project_id.clone(),
            name: s.name.clone(),
            status: ScenarioStatus::Draft,
            meta: serde_json::json!({}),
            steps: Vec::new(),
        };
        state.scenarios.insert(scenario.id.clone(), Record { scenario: scenario.clone() });
        Ok(scenario)
    }

    async fn update_scenario(
        &self,
        id: &str,
        name: &str,
        status: &str,
        meta: &serde_json::Value,
    ) -> Result<Option<ApiScenario>, RepoError> {
        let mut state = self.state.lock().expect("lock");
        let Some(rec) = state.scenarios.get_mut(id) else { return Ok(None) };
        rec.scenario.name = name.to_string();
        rec.scenario.status = ScenarioStatus::parse(status);
        rec.scenario.meta = meta.clone();
        let mut s = rec.scenario.clone();
        s.steps.sort_by_key(|st| st.order);
        Ok(Some(s))
    }

    async fn get_scenario(&self, id: &str) -> Result<Option<ApiScenario>, RepoError> {
        Ok(self.state.lock().expect("lock").scenarios.get(id).map(|r| {
            let mut s = r.scenario.clone();
            s.steps.sort_by_key(|st| st.order); // 按 order 升序
            s
        }))
    }

    async fn list_scenarios(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiScenario>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ApiScenario> = state
            .scenarios
            .values()
            .filter(|r| r.scenario.project_id == project_id)
            .map(|r| {
                let mut s = r.scenario.clone();
                s.steps.sort_by_key(|st| st.order);
                s
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id)); // 稳定顺序
        Ok(out)
    }

    async fn add_step(
        &self,
        scenario_id: &str,
        step: &NewScenarioStep,
    ) -> Result<ScenarioStep, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.step_seq += 1;
        let stored = ScenarioStep {
            id: format!("step-{}", state.step_seq),
            order: step.order,
            kind: step.kind.clone(),
            ref_mode: step.ref_mode,
            snapshot: step.snapshot.clone(),
        };
        if let Some(rec) = state.scenarios.get_mut(scenario_id) {
            rec.scenario.steps.push(stored.clone());
        }
        Ok(stored)
    }

    async fn delete_step(&self, scenario_id: &str, step_id: &str) -> Result<bool, RepoError> {
        let mut state = self.state.lock().expect("lock");
        let Some(rec) = state.scenarios.get_mut(scenario_id) else { return Ok(false) };
        let before = rec.scenario.steps.len();
        rec.scenario.steps.retain(|s| s.id != step_id);
        Ok(rec.scenario.steps.len() != before)
    }

    async fn record_execution(
        &self,
        scenario_id: &str,
        project_id: &str,
        status: &str,
        case_count: i32,
        report_id: Option<&str>,
    ) -> Result<ScenarioExecution, RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.exec_seq += 1;
        let n = state.exec_seq;
        // 未知状态回落到 Pending,保持落库可解析。
        let status = ExecutionStatus::parse(status).unwrap_or_default();
        let exec = ScenarioExecution {
            id: format!("exec-{n}"),
            scenario_id: scenario_id.to_string(),
            project_id: project_id.to_string(),
            status,
            case_count,
            report_id: report_id.map(|s| s.to_string()),
            // 合成递增的时间串,保证测试确定性且降序排序可用。
            created_at: format!("exec-{n}"),
        };
        state.executions.push(exec.clone());
        Ok(exec)
    }

    async fn count_executions(&self, scenario_id: &str) -> Result<u64, RepoError> {
        let state = self.state.lock().expect("lock");
        Ok(state.executions.iter().filter(|e| e.scenario_id == scenario_id).count() as u64)
    }

    async fn list_executions(
        &self,
        scenario_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<ScenarioExecution>, RepoError> {
        let state = self.state.lock().expect("lock");
        // created_at 降序(最新在前):exec_seq 递增,逆序遍历即可。
        let out: Vec<ScenarioExecution> = state
            .executions
            .iter()
            .rev()
            .filter(|e| e.scenario_id == scenario_id)
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(out)
    }

    async fn list_scenarios_referencing_cases(
        &self,
        case_ids: &[String],
    ) -> Result<Vec<ScenarioReference>, RepoError> {
        if case_ids.is_empty() {
            return Ok(Vec::new());
        }
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ScenarioReference> = state
            .scenarios
            .values()
            .filter(|r| {
                r.scenario.steps.iter().any(|st| match &st.kind {
                    StepKind::Case { case_id } => case_ids.iter().any(|c| c == case_id),
                    _ => false,
                })
            })
            .map(|r| ScenarioReference {
                id: r.scenario.id.clone(),
                project_id: r.scenario.project_id.clone(),
                name: r.scenario.name.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{
    ApiScenario, ExecutionStatus, NewApiScenario, NewScenarioStep, ScenarioChange,
    ScenarioExecution, ScenarioReference, ScenarioStatus, ScenarioStep, StepKind,
};
use crate::ports::{ApiScenarioRepository, RepoError};

#[derive(Clone)]
struct Record {
    scenario: ApiScenario,
}

#[derive(Default)]
struct State {
    scenarios: HashMap<String, Record>,
    scn_seq: u64,
    step_seq: u64,
    executions: Vec<ScenarioExecution>,
    exec_seq: u64,
    changes: Vec<ScenarioChange>,
    change_seq: u64,
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
            created_by: s.created_by.clone(),
            created_at: String::new(),
            updated_at: String::new(),
            steps: Vec::new(),
            last_result: None,
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
            s.steps.sort_by_key(|st| st.order);
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
        out.sort_by(|a, b| a.id.cmp(&b.id));
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

    async fn delete_scenario(&self, id: &str) -> Result<bool, RepoError> {
        Ok(self.state.lock().expect("lock").scenarios.remove(id).is_some())
    }

    async fn reorder_steps(&self, scenario_id: &str, ordered_ids: &[String]) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        let Some(rec) = state.scenarios.get_mut(scenario_id) else { return Ok(()) };
        for (i, id) in ordered_ids.iter().enumerate() {
            if let Some(st) = rec.scenario.steps.iter_mut().find(|s| &s.id == id) {
                st.order = (i + 1) as i32;
            }
        }
        Ok(())
    }

    async fn record_change(
        &self,
        scenario_id: &str,
        action: &str,
        detail: Option<&str>,
        user_id: Option<&str>,
    ) -> Result<(), RepoError> {
        let mut state = self.state.lock().expect("lock");
        state.change_seq += 1;
        let seq = state.change_seq;
        state.changes.push(ScenarioChange {
            id: format!("chg-{seq}"),
            scenario_id: scenario_id.to_string(),
            action: action.to_string(),
            detail: detail.map(|s| s.to_string()),
            user_id: user_id.map(|s| s.to_string()),
            created_at: format!("{seq:020}"),
        });
        Ok(())
    }

    async fn list_changes(&self, scenario_id: &str) -> Result<Vec<ScenarioChange>, RepoError> {
        let state = self.state.lock().expect("lock");
        let mut out: Vec<ScenarioChange> =
            state.changes.iter().filter(|c| c.scenario_id == scenario_id).cloned().collect();
        out.reverse();
        Ok(out)
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
        let status = ExecutionStatus::parse(status).unwrap_or_default();
        let exec = ScenarioExecution {
            id: format!("exec-{n}"),
            scenario_id: scenario_id.to_string(),
            project_id: project_id.to_string(),
            status,
            case_count,
            report_id: report_id.map(|s| s.to_string()),
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

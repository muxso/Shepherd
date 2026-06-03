//! 内存版场景仓储。Mutex 守护;id 形如 `scn-{n}` / `step-{n}`。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{
    ApiScenario, NewApiScenario, NewScenarioStep, ScenarioStatus, ScenarioStep,
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
            steps: Vec::new(),
        };
        state.scenarios.insert(scenario.id.clone(), Record { scenario: scenario.clone() });
        Ok(scenario)
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
}

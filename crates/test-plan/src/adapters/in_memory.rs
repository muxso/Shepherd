use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{CaseCounts, CaseResult, CaseStatus, NewPlan, Plan, PlanCase};
use crate::ports::{PlanRepository, RepoError};

#[derive(Default)]
struct State {
    plans: HashMap<String, Plan>,
    counts: HashMap<String, CaseCounts>,
    thresholds: HashMap<String, f64>,
    cases: HashMap<String, Vec<PlanCase>>,
    seq: u64,
}

#[derive(Clone, Default)]
pub struct InMemoryPlanRepository {
    state: Arc<Mutex<State>>,
}

impl InMemoryPlanRepository {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert_internal(&self, new_plan: &NewPlan) -> Plan {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.seq += 1;
        let plan = Plan {
            id: format!("plan-{}", state.seq),
            project_id: new_plan.project_id.clone(),
            name: new_plan.name.clone(),
            plan_type: new_plan.plan_type,
            group_id: new_plan.group_id.clone(),
            archived: false,
            // 无真实时钟:用单调递增的 seq 当创建时间,保证列表排序稳定。
            created_at_ms: state.seq as i64,
        };
        state.plans.insert(plan.id.clone(), plan.clone());
        plan
    }

    pub async fn seed(&self, new_plan: NewPlan) -> Plan {
        self.insert_internal(&new_plan)
    }

    pub fn set_counts(&self, plan_id: &str, counts: CaseCounts) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .counts
            .insert(plan_id.to_string(), counts);
    }

    pub fn set_threshold(&self, plan_id: &str, threshold: f64) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .thresholds
            .insert(plan_id.to_string(), threshold);
    }
}

#[async_trait]
impl PlanRepository for InMemoryPlanRepository {
    async fn insert(&self, new_plan: &NewPlan) -> Result<Plan, RepoError> {
        Ok(self.insert_internal(new_plan))
    }

    async fn get(&self, id: &str) -> Result<Option<Plan>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .plans
            .get(id)
            .cloned())
    }

    async fn children(&self, group_id: &str) -> Result<Vec<Plan>, RepoError> {
        let mut children: Vec<Plan> = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .plans
            .values()
            .filter(|p| p.group_id == group_id)
            .cloned()
            .collect();
        children.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(children)
    }

    async fn case_counts(&self, plan_id: &str) -> Result<CaseCounts, RepoError> {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match state.cases.get(plan_id) {
            Some(cases) if !cases.is_empty() => {
                let mut c = CaseCounts::default();
                for pc in cases {
                    match pc.status {
                        CaseStatus::Pending => c.pending += 1,
                        CaseStatus::Success => c.success += 1,
                        CaseStatus::Error => c.error += 1,
                        CaseStatus::FakeError => c.fake_error += 1,
                        CaseStatus::Block => c.block += 1,
                    }
                }
                Ok(c)
            }
            _ => Ok(state.counts.get(plan_id).copied().unwrap_or_default()),
        }
    }

    async fn pass_threshold(&self, plan_id: &str) -> Result<f64, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .thresholds
            .get(plan_id)
            .copied()
            .unwrap_or(0.0))
    }

    async fn link_case(&self, plan_id: &str, case_id: &str, name: &str) -> Result<(), RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let cases = state.cases.entry(plan_id.to_string()).or_default();
        if !cases.iter().any(|c| c.case_id == case_id) {
            cases.push(PlanCase {
                case_id: case_id.to_string(),
                name: name.to_string(),
                status: CaseStatus::Pending,
                result: None,
            });
        }
        Ok(())
    }

    async fn record_result(
        &self,
        plan_id: &str,
        case_id: &str,
        status: CaseStatus,
        result: Option<&CaseResult>,
    ) -> Result<bool, RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(cases) = state.cases.get_mut(plan_id) else { return Ok(false) };
        match cases.iter_mut().find(|c| c.case_id == case_id) {
            Some(pc) => {
                pc.status = status;
                pc.result = result.cloned();
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn list_cases(&self, plan_id: &str) -> Result<Vec<PlanCase>, RepoError> {
        Ok(self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cases
            .get(plan_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn list(&self, project_id: &str) -> Result<Vec<Plan>, RepoError> {
        let mut plans: Vec<Plan> = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .plans
            .values()
            .filter(|p| p.project_id == project_id && !p.archived)
            .cloned()
            .collect();
        plans.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(plans)
    }

    async fn rename(&self, id: &str, name: &str) -> Result<bool, RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match state.plans.get_mut(id) {
            Some(p) => {
                p.name = name.to_string();
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.cases.remove(id);
        Ok(state.plans.remove(id).is_some())
    }
}

#[derive(Default)]
pub struct InMemoryScheduleStore {
    schedules: Mutex<Vec<crate::domain::Schedule>>,
    runs: Mutex<Vec<crate::domain::PlanRun>>,
}

impl InMemoryScheduleStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl crate::ports::ScheduleStore for InMemoryScheduleStore {
    async fn insert(
        &self,
        s: &crate::domain::NewSchedule,
    ) -> Result<crate::domain::Schedule, RepoError> {
        let mut g = self.schedules.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        let view = crate::domain::Schedule {
            id: format!("s{}", g.len() + 1),
            plan_id: s.plan_id.clone(),
            cron: s.cron.clone(),
            enabled: s.enabled,
        };
        g.push(view.clone());
        Ok(view)
    }
    async fn list_enabled(&self) -> Result<Vec<crate::domain::Schedule>, RepoError> {
        Ok(self
            .schedules
            .lock()
            .map_err(|e| RepoError::Backend(e.to_string()))?
            .iter()
            .filter(|s| s.enabled)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl crate::ports::PlanRunStore for InMemoryScheduleStore {
    async fn record(
        &self,
        plan_id: &str,
        status: &str,
        total: u64,
        pass_rate: f64,
        execute_rate: f64,
    ) -> Result<crate::domain::PlanRun, RepoError> {
        let mut g = self.runs.lock().map_err(|e| RepoError::Backend(e.to_string()))?;
        let run = crate::domain::PlanRun {
            id: format!("r{}", g.len() + 1),
            plan_id: plan_id.to_string(),
            status: status.to_string(),
            total,
            pass_rate,
            execute_rate,
            triggered_at: "1970-01-01T00:00:00Z".to_string(),
        };
        g.push(run.clone());
        Ok(run)
    }
    async fn list_by_plan(
        &self,
        plan_id: &str,
        limit: u32,
    ) -> Result<Vec<crate::domain::PlanRun>, RepoError> {
        Ok(self
            .runs
            .lock()
            .map_err(|e| RepoError::Backend(e.to_string()))?
            .iter()
            .filter(|r| r.plan_id == plan_id)
            .rev()
            .take(limit as usize)
            .cloned()
            .collect())
    }
}

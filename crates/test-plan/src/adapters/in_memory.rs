//! 内存版计划仓储。可注入用例计数与通过阈值,供统计用例测试。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{CaseCounts, NewPlan, Plan};
use crate::ports::{PlanRepository, RepoError};

#[derive(Default)]
struct State {
    plans: HashMap<String, Plan>,
    counts: HashMap<String, CaseCounts>,
    thresholds: HashMap<String, f64>,
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
        let mut state = self.state.lock().expect("lock");
        state.seq += 1;
        let plan = Plan {
            id: format!("plan-{}", state.seq),
            project_id: new_plan.project_id.clone(),
            name: new_plan.name.clone(),
            plan_type: new_plan.plan_type,
            group_id: new_plan.group_id.clone(),
            archived: false,
        };
        state.plans.insert(plan.id.clone(), plan.clone());
        plan
    }

    // ---- 测试辅助 ----
    /// 直接落库一个计划(绕过用例校验,用于搭场景)。
    pub async fn seed(&self, new_plan: NewPlan) -> Plan {
        self.insert_internal(&new_plan)
    }

    pub fn set_counts(&self, plan_id: &str, counts: CaseCounts) {
        self.state.lock().expect("lock").counts.insert(plan_id.to_string(), counts);
    }

    pub fn set_threshold(&self, plan_id: &str, threshold: f64) {
        self.state.lock().expect("lock").thresholds.insert(plan_id.to_string(), threshold);
    }
}

#[async_trait]
impl PlanRepository for InMemoryPlanRepository {
    async fn insert(&self, new_plan: &NewPlan) -> Result<Plan, RepoError> {
        Ok(self.insert_internal(new_plan))
    }

    async fn get(&self, id: &str) -> Result<Option<Plan>, RepoError> {
        Ok(self.state.lock().expect("lock").plans.get(id).cloned())
    }

    async fn children(&self, group_id: &str) -> Result<Vec<Plan>, RepoError> {
        let mut children: Vec<Plan> = self
            .state
            .lock()
            .expect("lock")
            .plans
            .values()
            .filter(|p| p.group_id == group_id)
            .cloned()
            .collect();
        children.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(children)
    }

    async fn case_counts(&self, plan_id: &str) -> Result<CaseCounts, RepoError> {
        Ok(self.state.lock().expect("lock").counts.get(plan_id).copied().unwrap_or_default())
    }

    async fn pass_threshold(&self, plan_id: &str) -> Result<f64, RepoError> {
        Ok(self.state.lock().expect("lock").thresholds.get(plan_id).copied().unwrap_or(0.0))
    }
}

/// 内存定时调度 + 运行快照(测试)。同时实现 ScheduleStore 与 PlanRunStore。
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

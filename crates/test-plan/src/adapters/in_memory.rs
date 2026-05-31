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

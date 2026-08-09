//! Default LLM-free planner: one task per acceptance criterion, plus an integration task depending on all of them when there's more than one.

use async_trait::async_trait;

use crate::ports::{PlanError, PlannedTask, Planner, RequirementSpec};

#[derive(Clone, Default)]
pub struct HeuristicPlanner;

impl HeuristicPlanner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Planner for HeuristicPlanner {
    async fn plan(&self, spec: &RequirementSpec) -> Result<Vec<PlannedTask>, PlanError> {
        if spec.acceptance_criteria.is_empty() {
            return Ok(vec![PlannedTask {
                title: format!("implement {}", spec.title),
                description: spec.description.clone(),
                acceptance_criteria: Vec::new(),
                dependencies: Vec::new(),
            }]);
        }
        let mut tasks: Vec<PlannedTask> = spec
            .acceptance_criteria
            .iter()
            .map(|c| PlannedTask {
                title: format!("implement: {c}"),
                description: String::new(),
                acceptance_criteria: vec![c.clone()],
                dependencies: Vec::new(),
            })
            .collect();
        if tasks.len() > 1 {
            let deps: Vec<usize> = (0..tasks.len()).collect();
            tasks.push(PlannedTask {
                title: format!("integration verification: {}", spec.title),
                description: "integrate and verify all acceptance criteria".into(),
                acceptance_criteria: spec.acceptance_criteria.clone(),
                dependencies: deps,
            });
        }
        Ok(tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(criteria: &[&str]) -> RequirementSpec {
        RequirementSpec {
            requirement_id: "r1".into(),
            requirement_version: 1,
            title: "login".into(),
            description: "d".into(),
            acceptance_criteria: criteria.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn one_task_per_criterion_plus_integration() {
        let plan = HeuristicPlanner
            .plan(&spec(&["login success", "wrong password rejected"]))
            .await
            .expect("plan");
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[2].dependencies, vec![0, 1]);
        assert_eq!(plan[0].acceptance_criteria, vec!["login success".to_string()]);
    }

    #[tokio::test]
    async fn no_criteria_yields_single_task() {
        let plan = HeuristicPlanner.plan(&spec(&[])).await.expect("plan");
        assert_eq!(plan.len(), 1);
        assert!(plan[0].dependencies.is_empty());
    }
}

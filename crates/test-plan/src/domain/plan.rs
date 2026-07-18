/// "NONE" means the plan belongs to no plan group.
pub const ROOT_GROUP: &str = "NONE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanType {
    Plan,
    Group,
}

impl PlanType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlanType::Plan => "TEST_PLAN",
            PlanType::Group => "GROUP",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "TEST_PLAN" => Some(PlanType::Plan),
            "GROUP" => Some(PlanType::Group),
            _ => None,
        }
    }
}

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanError {
    #[error("plan name must not be empty")]
    EmptyName,
    #[error("a plan group cannot be nested in another group")]
    GroupCannotBeNested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlan {
    pub project_id: String,
    pub name: String,
    pub plan_type: PlanType,
    pub group_id: String,
}

impl NewPlan {
    pub fn new(
        project_id: &str,
        name: &str,
        plan_type: PlanType,
        group_id: &str,
    ) -> Result<Self, PlanError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(PlanError::EmptyName);
        }
        let group_id = group_id.trim();
        if plan_type == PlanType::Group && group_id != ROOT_GROUP {
            return Err(PlanError::GroupCannotBeNested);
        }
        Ok(Self {
            project_id: project_id.to_string(),
            name: name.to_string(),
            plan_type,
            group_id: group_id.to_string(),
        })
    }

    pub fn belongs_to_group(&self) -> bool {
        self.group_id != ROOT_GROUP
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub plan_type: PlanType,
    pub group_id: String,
    pub archived: bool,
    /// Unix milliseconds.
    pub created_at_ms: i64,
}

/// Editable plan fields beyond the core Plan identity (PUT /test-plan/{id}).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanMeta {
    pub description: String,
    pub tags: Vec<String>,
    pub module_id: Option<String>,
    pub group_id: String,
    /// Unix milliseconds; None = no start date.
    pub start_at_ms: Option<i64>,
    pub end_at_ms: Option<i64>,
    pub allow_duplicate_cases: bool,
    pub auto_update_status: bool,
    /// Fraction 0..=1 (the HTTP API exposes it as a 0..=100 percent).
    pub pass_threshold: f64,
}

impl Default for PlanMeta {
    fn default() -> Self {
        Self {
            description: String::new(),
            tags: Vec::new(),
            module_id: None,
            group_id: ROOT_GROUP.to_string(),
            start_at_ms: None,
            end_at_ms: None,
            allow_duplicate_cases: true,
            auto_update_status: true,
            pass_threshold: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_name() {
        assert_eq!(NewPlan::new("p1", "  ", PlanType::Plan, ROOT_GROUP), Err(PlanError::EmptyName));
    }

    #[test]
    fn group_at_root_is_ok() {
        let p = NewPlan::new("p1", "回归组", PlanType::Group, ROOT_GROUP).expect("ok");
        assert_eq!(p.plan_type, PlanType::Group);
        assert!(!p.belongs_to_group());
    }

    #[test]
    fn group_nested_in_another_group_is_rejected() {
        assert_eq!(
            NewPlan::new("p1", "组中组", PlanType::Group, "g123"),
            Err(PlanError::GroupCannotBeNested)
        );
    }

    #[test]
    fn plan_inside_group_is_ok() {
        let p = NewPlan::new("p1", "冒烟", PlanType::Plan, "g123").expect("ok");
        assert!(p.belongs_to_group());
    }

    #[test]
    fn type_string_roundtrip() {
        assert_eq!(PlanType::parse("GROUP"), Some(PlanType::Group));
        assert_eq!(PlanType::parse("test_plan"), Some(PlanType::Plan));
        assert_eq!(PlanType::parse("x"), None);
    }
}

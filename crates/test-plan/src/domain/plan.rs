//! 测试计划领域模型 + 计划组规则。

/// 根分组 id(MeterSphere 用 "NONE" 表示"不属于任何计划组")。
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
    /// 计划组不能再嵌进别的组(GROUP 的 groupId 必须是根)。
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
        // 计划组只能在根:GROUP 类型若指定了非根 groupId,非法。
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

    /// 是否归属某个计划组(而非挂在根下)。
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_name() {
        assert_eq!(
            NewPlan::new("p1", "  ", PlanType::Plan, ROOT_GROUP),
            Err(PlanError::EmptyName)
        );
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

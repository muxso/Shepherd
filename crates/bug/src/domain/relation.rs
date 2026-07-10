use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RelationError {
    #[error("unknown relation kind: {0}")]
    UnknownKind(String),
    #[error("relation target id must not be empty")]
    EmptyTarget,
}

/// 缺陷可关联的资产类型:需求 / 场景用例 / 功能用例。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Requirement,
    Scenario,
    FunctionalCase,
}

impl RelationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Requirement => "REQUIREMENT",
            Self::Scenario => "SCENARIO",
            Self::FunctionalCase => "FUNCTIONAL_CASE",
        }
    }

    pub fn parse(s: &str) -> Result<Self, RelationError> {
        match s.trim().to_ascii_uppercase().as_str() {
            "REQUIREMENT" => Ok(Self::Requirement),
            "SCENARIO" => Ok(Self::Scenario),
            "FUNCTIONAL_CASE" => Ok(Self::FunctionalCase),
            other => Err(RelationError::UnknownKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugRelation {
    pub bug_id: String,
    pub kind: RelationKind,
    pub target_id: String,
}

impl BugRelation {
    pub fn new(bug_id: &str, kind: &str, target_id: &str) -> Result<Self, RelationError> {
        let target_id = target_id.trim();
        if target_id.is_empty() {
            return Err(RelationError::EmptyTarget);
        }
        Ok(Self {
            bug_id: bug_id.to_string(),
            kind: RelationKind::parse(kind)?,
            target_id: target_id.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kind_is_case_insensitive() {
        assert_eq!(RelationKind::parse("requirement"), Ok(RelationKind::Requirement));
        assert_eq!(RelationKind::parse(" SCENARIO "), Ok(RelationKind::Scenario));
        assert_eq!(RelationKind::parse("functional_case"), Ok(RelationKind::FunctionalCase));
        assert!(matches!(RelationKind::parse("king"), Err(RelationError::UnknownKind(_))));
    }

    #[test]
    fn new_rejects_empty_target() {
        assert_eq!(BugRelation::new("b1", "REQUIREMENT", "  "), Err(RelationError::EmptyTarget));
        let r = BugRelation::new("b1", "SCENARIO", " s1 ").expect("valid");
        assert_eq!(r.target_id, "s1");
        assert_eq!(r.kind.as_str(), "SCENARIO");
    }
}

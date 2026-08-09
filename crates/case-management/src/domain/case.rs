use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseStep {
    pub step: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CaseError {
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("case name must not be empty")]
    EmptyName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionalCase {
    pub id: String,
    pub project_id: String,
    /// Per-project display number shown as the case ID in the UI (100001…).
    pub num: i64,
    pub name: String,
    pub module: String,
    pub priority: String,
    pub status: String,
    pub tags: Vec<String>,
    pub custom_fields: BTreeMap<String, String>,
    pub steps: Vec<CaseStep>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFunctionalCase {
    pub project_id: String,
    pub name: String,
    pub module: String,
    pub priority: String,
    pub status: String,
    pub tags: Vec<String>,
    pub custom_fields: BTreeMap<String, String>,
    pub steps: Vec<CaseStep>,
    pub created_by: Option<String>,
}

impl NewFunctionalCase {
    pub fn new(
        project_id: &str,
        name: &str,
        module: &str,
        priority: &str,
        status: &str,
        custom_fields: BTreeMap<String, String>,
        steps: Vec<CaseStep>,
    ) -> Result<Self, CaseError> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(CaseError::EmptyProject);
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(CaseError::EmptyName);
        }
        let with_default = |v: &str, d: &str| {
            let v = v.trim();
            if v.is_empty() {
                d.to_string()
            } else {
                v.to_string()
            }
        };
        Ok(Self {
            project_id: project_id.to_string(),
            name: name.to_string(),
            module: module.trim().to_string(),
            priority: with_default(priority, "P2"),
            status: with_default(status, "PREPARED"),
            tags: Vec::new(),
            custom_fields,
            steps,
            created_by: None,
        })
    }

    pub fn with_created_by(mut self, user_id: Option<&str>) -> Self {
        self.created_by = user_id.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags =
            tags.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> BTreeMap<String, String> {
        BTreeMap::from([("owner".into(), "alice".into())])
    }

    #[test]
    fn defaults_priority_and_status() {
        let c = NewFunctionalCase::new("p1", "login success", "", "", "", fields(), Vec::new())
            .expect("ok");
        assert_eq!(c.priority, "P2");
        assert_eq!(c.status, "PREPARED");
        assert_eq!(c.custom_fields["owner"], "alice");
    }

    #[test]
    fn keeps_given_values_and_trims() {
        let c = NewFunctionalCase::new(
            " p1 ",
            " case ",
            "login",
            "P0",
            "REVIEWING",
            fields(),
            Vec::new(),
        )
        .expect("ok");
        assert_eq!(c.project_id, "p1");
        assert_eq!(c.name, "case");
        assert_eq!(c.priority, "P0");
        assert_eq!(c.status, "REVIEWING");
    }

    #[test]
    fn rejects_empty_project_and_name() {
        assert_eq!(
            NewFunctionalCase::new(" ", "x", "", "", "", fields(), Vec::new()).unwrap_err(),
            CaseError::EmptyProject
        );
        assert_eq!(
            NewFunctionalCase::new("p1", "  ", "", "", "", fields(), Vec::new()).unwrap_err(),
            CaseError::EmptyName
        );
    }
}

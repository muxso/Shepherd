use thiserror::Error;

/// Max project-name length (matches the DB column).
pub const MAX_NAME_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProjectError {
    #[error("project name must not be empty")]
    EmptyName,
    #[error("project name too long")]
    NameTooLong,
    #[error("organization id must not be empty")]
    EmptyOrganization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProject {
    pub organization_id: String,
    pub name: String,
}

impl NewProject {
    pub fn new(organization_id: &str, name: &str) -> Result<Self, ProjectError> {
        let organization_id = organization_id.trim();
        if organization_id.is_empty() {
            return Err(ProjectError::EmptyOrganization);
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(ProjectError::EmptyName);
        }
        // Count chars, not bytes, so multibyte (e.g. Chinese) names aren't falsely over-length.
        if name.chars().count() > MAX_NAME_LEN {
            return Err(ProjectError::NameTooLong);
        }
        Ok(Self { organization_id: organization_id.to_string(), name: name.to_string() })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub organization_id: String,
    pub name: String,
    pub enable: bool,
    pub deleted: bool,
}

impl Project {
    pub fn active(id: &str, organization_id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            organization_id: organization_id.to_string(),
            name: name.to_string(),
            enable: true,
            deleted: false,
        }
    }

    pub fn enable(&mut self) {
        self.enable = true;
    }

    pub fn disable(&mut self) {
        self.enable = false;
    }

    pub fn soft_delete(&mut self) {
        self.deleted = true;
    }

    /// Only non-deleted projects occupy their name (after soft delete the name can be reused).
    pub fn occupies_name(&self) -> bool {
        !self.deleted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_trims_name_and_org() {
        let p = NewProject::new("  org1 ", "  Demo  ").expect("valid");
        assert_eq!(p.organization_id, "org1");
        assert_eq!(p.name, "Demo");
    }

    #[test]
    fn rejects_blank_name() {
        assert_eq!(NewProject::new("org1", "   "), Err(ProjectError::EmptyName));
    }

    #[test]
    fn rejects_blank_org() {
        assert_eq!(NewProject::new("  ", "Demo"), Err(ProjectError::EmptyOrganization));
    }

    #[test]
    fn rejects_overlong_name() {
        let long = "x".repeat(MAX_NAME_LEN + 1);
        assert_eq!(NewProject::new("org1", &long), Err(ProjectError::NameTooLong));
    }

    #[test]
    fn accepts_255_chars_including_cjk() {
        let cjk = "项".repeat(MAX_NAME_LEN);
        assert!(NewProject::new("org1", &cjk).is_ok());
    }

    #[test]
    fn active_project_defaults() {
        let p = Project::active("p1", "org1", "Demo");
        assert!(p.enable);
        assert!(!p.deleted);
        assert!(p.occupies_name());
    }

    #[test]
    fn disable_then_enable() {
        let mut p = Project::active("p1", "org1", "Demo");
        p.disable();
        assert!(!p.enable);
        p.enable();
        assert!(p.enable);
    }

    #[test]
    fn soft_deleted_project_frees_its_name() {
        let mut p = Project::active("p1", "org1", "Demo");
        p.soft_delete();
        assert!(p.deleted);
        assert!(!p.occupies_name());
    }
}

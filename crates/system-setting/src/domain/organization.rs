use thiserror::Error;

pub const MAX_NAME_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrgError {
    #[error("organization name must not be empty")]
    EmptyName,
    #[error("organization name too long")]
    NameTooLong,
}

fn validate_name(name: &str) -> Result<String, OrgError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OrgError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(OrgError::NameTooLong);
    }
    Ok(name.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOrganization {
    pub name: String,
}

impl NewOrganization {
    pub fn new(name: &str) -> Result<Self, OrgError> {
        Ok(Self { name: validate_name(name)? })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub enable: bool,
    pub deleted: bool,
}

impl Organization {
    pub fn active(id: &str, name: &str) -> Self {
        Self { id: id.to_string(), name: name.to_string(), enable: true, deleted: false }
    }

    pub fn rename(&mut self, name: &str) -> Result<(), OrgError> {
        self.name = validate_name(name)?;
        Ok(())
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enable = on;
    }

    pub fn soft_delete(&mut self) {
        self.deleted = true;
    }

    /// Names are reusable after soft delete, so only non-deleted orgs occupy uniqueness.
    pub fn occupies_name(&self) -> bool {
        !self.deleted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_and_overlong() {
        assert_eq!(NewOrganization::new("  "), Err(OrgError::EmptyName));
        assert_eq!(NewOrganization::new(&"x".repeat(MAX_NAME_LEN + 1)), Err(OrgError::NameTooLong));
    }

    #[test]
    fn rename_validates_and_trims() {
        let mut o = Organization::active("o1", "Acme");
        o.rename("  NewName ").expect("ok");
        assert_eq!(o.name, "NewName");
        assert_eq!(o.rename(""), Err(OrgError::EmptyName));
    }

    #[test]
    fn soft_delete_frees_name() {
        let mut o = Organization::active("o1", "Acme");
        assert!(o.occupies_name());
        o.soft_delete();
        assert!(o.deleted && !o.occupies_name());
    }
}

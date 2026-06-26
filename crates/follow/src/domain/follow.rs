use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FollowError {
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("entity type must not be empty")]
    EmptyEntityType,
    #[error("entity id must not be empty")]
    EmptyEntityId,
    #[error("user id must not be empty")]
    EmptyUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Follow {
    pub project_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub user_id: String,
}

impl Follow {
    pub fn new(
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<Self, FollowError> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(FollowError::EmptyProject);
        }
        let entity_type = entity_type.trim();
        if entity_type.is_empty() {
            return Err(FollowError::EmptyEntityType);
        }
        let entity_id = entity_id.trim();
        if entity_id.is_empty() {
            return Err(FollowError::EmptyEntityId);
        }
        let user_id = user_id.trim();
        if user_id.is_empty() {
            return Err(FollowError::EmptyUser);
        }
        Ok(Self {
            project_id: project_id.to_string(),
            entity_type: entity_type.to_ascii_lowercase(),
            entity_id: entity_id.to_string(),
            user_id: user_id.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_trims_and_lowercases_entity_type() {
        let f = Follow::new(" p1 ", "  Bug ", " b-1 ", " alice ").expect("valid");
        assert_eq!(f.project_id, "p1");
        assert_eq!(f.entity_type, "bug");
        assert_eq!(f.entity_id, "b-1");
        assert_eq!(f.user_id, "alice");
    }

    #[test]
    fn rejects_blank_fields() {
        assert_eq!(Follow::new(" ", "bug", "b1", "u"), Err(FollowError::EmptyProject));
        assert_eq!(Follow::new("p", "  ", "b1", "u"), Err(FollowError::EmptyEntityType));
        assert_eq!(Follow::new("p", "bug", " ", "u"), Err(FollowError::EmptyEntityId));
        assert_eq!(Follow::new("p", "bug", "b1", "  "), Err(FollowError::EmptyUser));
    }
}

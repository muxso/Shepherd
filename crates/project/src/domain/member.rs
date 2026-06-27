use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemberError {
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("user id must not be empty")]
    EmptyUser,
    #[error("unknown member role: {0}")]
    InvalidRole(String),
}

/// 项目角色。授权仍由全局 RBAC 决定;这里只是项目内的成员身份标注。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRole {
    Owner,
    Member,
    Viewer,
}

impl MemberRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "OWNER",
            Self::Member => "MEMBER",
            Self::Viewer => "VIEWER",
        }
    }

    pub fn parse(s: &str) -> Result<Self, MemberError> {
        match s.trim().to_ascii_uppercase().as_str() {
            "OWNER" => Ok(Self::Owner),
            "MEMBER" => Ok(Self::Member),
            "VIEWER" => Ok(Self::Viewer),
            other => Err(MemberError::InvalidRole(other.to_string())),
        }
    }
}

/// 入参校验后的成员归属(尚未落库)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMember {
    pub project_id: String,
    pub user_id: String,
    pub role: MemberRole,
}

impl NewMember {
    pub fn new(project_id: &str, user_id: &str, role: &str) -> Result<Self, MemberError> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(MemberError::EmptyProject);
        }
        let user_id = user_id.trim();
        if user_id.is_empty() {
            return Err(MemberError::EmptyUser);
        }
        Ok(Self {
            project_id: project_id.to_string(),
            user_id: user_id.to_string(),
            role: MemberRole::parse(role)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMember {
    pub project_id: String,
    pub user_id: String,
    pub role: MemberRole,
    pub added_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_parse_is_case_insensitive_and_roundtrips() {
        assert_eq!(MemberRole::parse("owner").expect("ok"), MemberRole::Owner);
        assert_eq!(MemberRole::parse("  Member ").expect("ok"), MemberRole::Member);
        assert_eq!(MemberRole::Viewer.as_str(), "VIEWER");
        assert_eq!(MemberRole::parse("VIEWER").expect("ok").as_str(), "VIEWER");
    }

    #[test]
    fn unknown_role_is_rejected() {
        assert_eq!(MemberRole::parse("boss"), Err(MemberError::InvalidRole("BOSS".into())));
    }

    #[test]
    fn new_member_trims_and_validates() {
        let m = NewMember::new("  p1 ", " u1 ", "owner").expect("valid");
        assert_eq!(m.project_id, "p1");
        assert_eq!(m.user_id, "u1");
        assert_eq!(m.role, MemberRole::Owner);
    }

    #[test]
    fn new_member_rejects_blank_and_bad_role() {
        assert_eq!(NewMember::new(" ", "u", "MEMBER"), Err(MemberError::EmptyProject));
        assert_eq!(NewMember::new("p", "  ", "MEMBER"), Err(MemberError::EmptyUser));
        assert_eq!(NewMember::new("p", "u", "king"), Err(MemberError::InvalidRole("KING".into())));
    }
}

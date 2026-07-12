use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UserError {
    #[error("user name must not be empty")]
    EmptyName,
    #[error("invalid email address")]
    InvalidEmail,
}

impl Email {
    pub fn parse(raw: &str) -> Result<Self, UserError> {
        let raw = raw.trim();
        let (local, domain) = raw.split_once('@').ok_or(UserError::InvalidEmail)?;
        if local.is_empty() || domain.is_empty() || domain.contains('@') || !domain.contains('.') {
            return Err(UserError::InvalidEmail);
        }
        Ok(Self(raw.to_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUser {
    pub name: String,
    pub email: Email,
}

impl NewUser {
    pub fn new(name: &str, email: &str) -> Result<Self, UserError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(UserError::EmptyName);
        }
        Ok(Self { name: name.to_string(), email: Email::parse(email)? })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Email,
    pub enable: bool,
    pub deleted: bool,
}

impl User {
    pub fn rename(&mut self, name: &str) -> Result<(), UserError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(UserError::EmptyName);
        }
        self.name = name.to_string();
        Ok(())
    }

    pub fn set_email(&mut self, raw: &str) -> Result<(), UserError> {
        self.email = Email::parse(raw)?;
        Ok(())
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enable = on;
    }

    pub fn soft_delete(&mut self) {
        self.deleted = true;
    }

    /// Only non-deleted users occupy email uniqueness; emails are reusable after soft delete.
    pub fn occupies_email(&self) -> bool {
        !self.deleted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_email_and_lowercases() {
        assert_eq!(Email::parse("Alice@Example.COM").expect("valid").as_str(), "alice@example.com");
    }

    #[test]
    fn rejects_bad_emails() {
        assert_eq!(Email::parse("alice.example.com"), Err(UserError::InvalidEmail));
        assert_eq!(Email::parse("alice@localhost"), Err(UserError::InvalidEmail));
        assert_eq!(Email::parse("a@b@c.com"), Err(UserError::InvalidEmail));
    }

    #[test]
    fn new_user_rejects_blank_name_and_trims() {
        assert_eq!(NewUser::new("  ", "a@x.com"), Err(UserError::EmptyName));
        assert_eq!(NewUser::new("  Alice  ", "a@x.com").expect("valid").name, "Alice");
    }

    #[test]
    fn error_displays_message() {
        assert_eq!(UserError::InvalidEmail.to_string(), "invalid email address");
    }
}

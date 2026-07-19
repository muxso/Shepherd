use thiserror::Error;

/// Sidebar categories of the message center.
pub const CATEGORIES: &[&str] = &["PLAN", "BUG", "CASE", "API", "SCHEDULE"];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NoticeError {
    #[error("notice needs at least one receiver")]
    NoReceivers,
    #[error("notice category must not be empty")]
    EmptyCategory,
    #[error("notice title must not be empty")]
    EmptyTitle,
}

/// A notification to fan out: one stored row per receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNotice {
    pub project_id: String,
    pub receivers: Vec<String>,
    pub category: String,
    pub event_type: String,
    pub title: String,
    pub content: String,
    pub resource_type: String,
    pub resource_id: String,
    pub operator: String,
    pub at_mention: bool,
}

impl NewNotice {
    /// Trims and dedups receivers, validates category/title.
    pub fn validated(mut self) -> Result<Self, NoticeError> {
        let mut receivers: Vec<String> =
            self.receivers.iter().map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect();
        receivers.sort();
        receivers.dedup();
        if receivers.is_empty() {
            return Err(NoticeError::NoReceivers);
        }
        self.receivers = receivers;
        self.category = self.category.trim().to_string();
        if self.category.is_empty() {
            return Err(NoticeError::EmptyCategory);
        }
        self.title = self.title.trim().to_string();
        if self.title.is_empty() {
            return Err(NoticeError::EmptyTitle);
        }
        Ok(self)
    }
}

/// A stored notification (per receiver). created_at is epoch millis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub id: String,
    pub project_id: String,
    pub receiver_id: String,
    pub category: String,
    pub event_type: String,
    pub title: String,
    pub content: String,
    pub resource_type: String,
    pub resource_id: String,
    pub operator: String,
    pub at_mention: bool,
    pub read: bool,
    pub created_at: i64,
}

/// Maps a comment target entity type to a message category.
pub fn category_for_entity(entity_type: &str) -> String {
    match entity_type.trim().to_ascii_uppercase().as_str() {
        "BUG" => "BUG".to_string(),
        "TEST_PLAN" | "PLAN" => "PLAN".to_string(),
        "FUNCTIONAL_CASE" | "CASE_REVIEW" | "CASE" => "CASE".to_string(),
        s if s.starts_with("API") || s == "SCENARIO" => "API".to_string(),
        other => other.to_string(),
    }
}

fn is_mention_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

/// Extracts `@username` candidates from free text (dedup, in order of appearance).
/// A name is a run of alphanumeric / `_` / `-` / `.` chars (covers ids like `u-admin`).
pub fn parse_mentions(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, c) in text.char_indices() {
        if c != '@' {
            continue;
        }
        let rest = &text[i + c.len_utf8()..];
        let name: String = rest.chars().take_while(|&c| is_mention_char(c)).collect();
        // Trailing dots are sentence punctuation, not part of the name.
        let name = name.trim_end_matches('.').to_string();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> NewNotice {
        NewNotice {
            project_id: "p1".into(),
            receivers: vec!["u1".into()],
            category: "BUG".into(),
            event_type: "BUG_ASSIGNED".into(),
            title: "boom".into(),
            content: String::new(),
            resource_type: "BUG".into(),
            resource_id: "b1".into(),
            operator: "admin".into(),
            at_mention: false,
        }
    }

    #[test]
    fn validated_dedups_and_trims_receivers() {
        let mut n = base();
        n.receivers = vec![" u1 ".into(), "u1".into(), "".into(), "u2".into()];
        let n = n.validated().expect("valid");
        assert_eq!(n.receivers, vec!["u1", "u2"]);
    }

    #[test]
    fn validated_rejects_empty_receivers_and_blank_fields() {
        let mut n = base();
        n.receivers = vec!["  ".into()];
        assert_eq!(n.validated(), Err(NoticeError::NoReceivers));
        let mut n = base();
        n.category = " ".into();
        assert_eq!(n.validated(), Err(NoticeError::EmptyCategory));
        let mut n = base();
        n.title = "".into();
        assert_eq!(n.validated(), Err(NoticeError::EmptyTitle));
    }

    #[test]
    fn parse_mentions_extracts_names() {
        assert_eq!(parse_mentions("cc @admin 和 @u-admin, 看下"), vec!["admin", "u-admin"]);
        assert_eq!(parse_mentions("@a.b. end"), vec!["a.b"]);
        assert_eq!(parse_mentions("mail me at x@example.com"), vec!["example.com"]);
        assert!(parse_mentions("no mentions @ all").is_empty());
        // Duplicates collapse.
        assert_eq!(parse_mentions("@bob @bob"), vec!["bob"]);
    }

    #[test]
    fn category_for_entity_maps_known_types() {
        assert_eq!(category_for_entity("BUG"), "BUG");
        assert_eq!(category_for_entity("functional_case"), "CASE");
        assert_eq!(category_for_entity("CASE_REVIEW"), "CASE");
        assert_eq!(category_for_entity("TEST_PLAN"), "PLAN");
        assert_eq!(category_for_entity("API_CASE"), "API");
        assert_eq!(category_for_entity("SCENARIO"), "API");
        assert_eq!(category_for_entity("REQUIREMENT"), "REQUIREMENT");
    }
}

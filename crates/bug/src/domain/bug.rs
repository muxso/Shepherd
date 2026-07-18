use std::collections::BTreeMap;

use crate::domain::status_flow::StatusFlowGraph;

use thiserror::Error;

/// Custom field limits: max key count / key length / value length. Lengths count
/// chars, not bytes, so CJK text isn't over-counted.
pub const MAX_CUSTOM_FIELDS: usize = 32;
pub const MAX_CUSTOM_FIELD_KEY_LEN: usize = 64;
pub const MAX_CUSTOM_FIELD_VALUE_LEN: usize = 2000;

/// Allowed severity levels (mirrors functional-case priority P0..P3).
pub const SEVERITIES: [&str; 4] = ["P0", "P1", "P2", "P3"];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BugError {
    #[error("bug title must not be empty")]
    EmptyTitle,
    #[error("unknown target status: {0}")]
    UnknownStatus(String),
    #[error("transition not allowed: {from} -> {to}")]
    TransitionNotAllowed { from: String, to: String },
    #[error("too many custom fields (max {MAX_CUSTOM_FIELDS})")]
    TooManyCustomFields,
    #[error("custom field key must not be empty")]
    EmptyCustomFieldKey,
    #[error("custom field key too long: {0}")]
    CustomFieldKeyTooLong(String),
    #[error("custom field value too long for key: {0}")]
    CustomFieldValueTooLong(String),
    #[error("invalid severity: {0} (expected P0..P3)")]
    InvalidSeverity(String),
}

/// Normalizes an optional severity: trims, uppercases, blank → None; anything
/// outside `SEVERITIES` is rejected.
pub fn normalize_severity(raw: Option<&str>) -> Result<Option<String>, BugError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => {
            let up = s.to_uppercase();
            if SEVERITIES.contains(&up.as_str()) {
                Ok(Some(up))
            } else {
                Err(BugError::InvalidSeverity(s.to_string()))
            }
        }
    }
}

/// Normalizes an optional handler (user id): trims, blank → None.
pub fn normalize_handler(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Normalizes custom fields: keys trimmed, must be non-empty and ≤ `MAX_CUSTOM_FIELD_KEY_LEN`
/// chars; values ≤ `MAX_CUSTOM_FIELD_VALUE_LEN` chars; at most `MAX_CUSTOM_FIELDS` keys
/// (all lengths in chars, not bytes).
pub fn normalize_custom_fields(
    raw: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, BugError> {
    let mut out = BTreeMap::new();
    for (k, v) in raw {
        let k = k.trim();
        if k.is_empty() {
            return Err(BugError::EmptyCustomFieldKey);
        }
        if k.chars().count() > MAX_CUSTOM_FIELD_KEY_LEN {
            return Err(BugError::CustomFieldKeyTooLong(k.to_string()));
        }
        if v.chars().count() > MAX_CUSTOM_FIELD_VALUE_LEN {
            return Err(BugError::CustomFieldValueTooLong(k.to_string()));
        }
        out.insert(k.to_string(), v.clone());
    }
    if out.len() > MAX_CUSTOM_FIELDS {
        return Err(BugError::TooManyCustomFields);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBug {
    pub project_id: String,
    pub title: String,
    pub created_by: Option<String>,
    /// Validated severity (see `normalize_severity`), P0..P3.
    pub severity: Option<String>,
    /// Handler user id.
    pub handler: Option<String>,
    /// Validated custom field values (see `normalize_custom_fields`); field definitions
    /// live in the project template.
    pub custom_fields: BTreeMap<String, String>,
}

impl NewBug {
    pub fn new(project_id: &str, title: &str) -> Result<Self, BugError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(BugError::EmptyTitle);
        }
        Ok(Self {
            project_id: project_id.to_string(),
            title: title.to_string(),
            created_by: None,
            severity: None,
            handler: None,
            custom_fields: BTreeMap::new(),
        })
    }

    pub fn with_created_by(mut self, user_id: Option<&str>) -> Self {
        self.created_by = user_id.map(|s| s.to_string());
        self
    }

    /// Attaches a validated severity (caller runs `normalize_severity` first).
    pub fn with_severity(mut self, severity: Option<String>) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_handler(mut self, handler: Option<String>) -> Self {
        self.handler = handler;
        self
    }

    /// Attaches validated custom fields (caller runs `normalize_custom_fields` first).
    pub fn with_custom_fields(mut self, custom_fields: BTreeMap<String, String>) -> Self {
        self.custom_fields = custom_fields;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bug {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub deleted: bool,
    pub created_at: i64,
    pub created_by: Option<String>,
    /// Severity level P0..P3 (P0 highest).
    pub severity: Option<String>,
    /// Handler user id.
    pub handler: Option<String>,
    /// Last mutator's user id; stamped by every mutation, seeded from created_by on insert.
    pub updated_by: Option<String>,
    /// Last mutation time as timestamptz text (adapter-formatted).
    pub updated_at: Option<String>,
    /// Custom field values, key → string value; field definitions live in the project
    /// template, multi-select values are comma-joined.
    pub custom_fields: BTreeMap<String, String>,
}

impl Bug {
    pub fn change_status(&mut self, to: &str, flow: &StatusFlowGraph) -> Result<(), BugError> {
        if !flow.contains(to) {
            return Err(BugError::UnknownStatus(to.to_string()));
        }
        if !flow.can_transition(&self.status, to) {
            return Err(BugError::TransitionNotAllowed {
                from: self.status.clone(),
                to: to.to_string(),
            });
        }
        self.status = to.to_string();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::status_flow::StatusItem;

    fn flow() -> StatusFlowGraph {
        StatusFlowGraph::new(
            vec![
                StatusItem::new("NEW", "新建", true),
                StatusItem::new("RESOLVED", "已解决", true),
                StatusItem::new("CLOSED", "已关闭", true),
            ],
            vec![("NEW".into(), "RESOLVED".into()), ("RESOLVED".into(), "CLOSED".into())],
        )
    }

    fn bug_at(status: &str) -> Bug {
        Bug {
            id: "b1".into(),
            project_id: "p1".into(),
            title: "boom".into(),
            status: status.into(),
            deleted: false,
            created_at: 0,
            created_by: None,
            severity: None,
            handler: None,
            updated_by: None,
            updated_at: None,
            custom_fields: BTreeMap::new(),
        }
    }

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn new_bug_rejects_blank_title() {
        assert_eq!(NewBug::new("p1", "  "), Err(BugError::EmptyTitle));
    }

    #[test]
    fn new_bug_carries_created_by() {
        let nb = NewBug::new("p1", "boom").expect("valid").with_created_by(Some("alice"));
        assert_eq!(nb.created_by.as_deref(), Some("alice"));
        assert_eq!(NewBug::new("p1", "boom").expect("valid").created_by, None);
    }

    #[test]
    fn new_bug_carries_severity_and_handler() {
        let nb = NewBug::new("p1", "boom")
            .expect("valid")
            .with_severity(Some("P1".into()))
            .with_handler(Some("bob".into()));
        assert_eq!(nb.severity.as_deref(), Some("P1"));
        assert_eq!(nb.handler.as_deref(), Some("bob"));
        let plain = NewBug::new("p1", "boom").expect("valid");
        assert_eq!(plain.severity, None);
        assert_eq!(plain.handler, None);
    }

    #[test]
    fn normalize_severity_accepts_p0_to_p3_case_insensitive() {
        assert_eq!(normalize_severity(Some("P0")), Ok(Some("P0".into())));
        assert_eq!(normalize_severity(Some(" p3 ")), Ok(Some("P3".into())));
        assert_eq!(normalize_severity(None), Ok(None));
        assert_eq!(normalize_severity(Some("  ")), Ok(None));
        assert_eq!(
            normalize_severity(Some("P9")),
            Err(BugError::InvalidSeverity("P9".to_string()))
        );
        assert_eq!(
            normalize_severity(Some("CRITICAL")),
            Err(BugError::InvalidSeverity("CRITICAL".to_string()))
        );
    }

    #[test]
    fn normalize_handler_trims_and_blanks_to_none() {
        assert_eq!(normalize_handler(Some(" bob ")), Some("bob".to_string()));
        assert_eq!(normalize_handler(Some("   ")), None);
        assert_eq!(normalize_handler(None), None);
    }

    #[test]
    fn new_bug_carries_custom_fields_default_empty() {
        let nb = NewBug::new("p1", "boom")
            .expect("valid")
            .with_custom_fields(fields(&[("severity", "P0")]));
        assert_eq!(nb.custom_fields, fields(&[("severity", "P0")]));
        assert!(NewBug::new("p1", "boom").expect("valid").custom_fields.is_empty());
    }

    #[test]
    fn normalize_custom_fields_trims_keys_and_keeps_values() {
        let out = normalize_custom_fields(&fields(&[(" severity ", "P0")])).expect("ok");
        assert_eq!(out, fields(&[("severity", "P0")]));
        assert_eq!(normalize_custom_fields(&BTreeMap::new()), Ok(BTreeMap::new()));
    }

    #[test]
    fn normalize_custom_fields_rejects_blank_or_long_key() {
        assert_eq!(
            normalize_custom_fields(&fields(&[("  ", "v")])),
            Err(BugError::EmptyCustomFieldKey)
        );
        let long_key = "键".repeat(65);
        assert_eq!(
            normalize_custom_fields(&fields(&[(long_key.as_str(), "v")])),
            Err(BugError::CustomFieldKeyTooLong(long_key))
        );
        // 64 chars (CJK included) is legal.
        assert!(normalize_custom_fields(&fields(&[("键".repeat(64).as_str(), "v")])).is_ok());
    }

    #[test]
    fn normalize_custom_fields_rejects_long_value_or_too_many_keys() {
        assert_eq!(
            normalize_custom_fields(&fields(&[("k", "值".repeat(2001).as_str())])),
            Err(BugError::CustomFieldValueTooLong("k".to_string()))
        );
        assert!(normalize_custom_fields(&fields(&[("k", "值".repeat(2000).as_str())])).is_ok());
        let many: BTreeMap<String, String> =
            (0..33).map(|i| (format!("k{i}"), "v".to_string())).collect();
        assert_eq!(normalize_custom_fields(&many), Err(BugError::TooManyCustomFields));
        let ok: BTreeMap<String, String> =
            (0..32).map(|i| (format!("k{i}"), "v".to_string())).collect();
        assert_eq!(normalize_custom_fields(&ok).expect("ok").len(), 32);
    }

    #[test]
    fn valid_transition_updates_status() {
        let mut b = bug_at("NEW");
        b.change_status("RESOLVED", &flow()).expect("allowed");
        assert_eq!(b.status, "RESOLVED");
    }

    #[test]
    fn disallowed_transition_is_rejected_and_status_unchanged() {
        let mut b = bug_at("NEW");
        let err = b.change_status("CLOSED", &flow()).unwrap_err();
        assert_eq!(err, BugError::TransitionNotAllowed { from: "NEW".into(), to: "CLOSED".into() });
        assert_eq!(b.status, "NEW");
    }

    #[test]
    fn unknown_target_status_is_rejected() {
        let mut b = bug_at("NEW");
        let err = b.change_status("GHOST", &flow()).unwrap_err();
        assert_eq!(err, BugError::UnknownStatus("GHOST".into()));
        assert_eq!(b.status, "NEW");
    }
}

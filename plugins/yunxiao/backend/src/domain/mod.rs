use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const KINDS: &[&str] = &["requirement", "bug", "sprint", "commit", "testcase"];
#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("storage error")]
    Storage,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub kind: String,
    pub external_id: String,
    pub data: Value,
}
impl Record {
    pub fn validate(&self) -> Result<(), Error> {
        if !KINDS.contains(&self.kind.as_str())
            || self.external_id.trim().is_empty()
            || self.external_id.len() > 300
            || !self.data.is_object()
        {
            return Err(Error::Invalid("invalid external record".into()));
        }
        for key in ["createdAt", "updatedAt", "statusChangedAt", "authoredAt"] {
            if let Some(s) = self.data.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()) {
                chrono::DateTime::parse_from_rfc3339(s)
                    .map_err(|_| Error::Invalid(format!("invalid {key}")))?;
            }
        }
        if let Some(s) = self.data.get("due").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| Error::Invalid("invalid due date".into()))?;
        }
        if self.kind == "commit" {
            if self.data.get("isMerge").and_then(Value::as_bool) == Some(true)
                || self.data.get("parentIds").and_then(Value::as_array).is_some_and(|p| p.len() > 1)
            {
                return Err(Error::Invalid("merge commits are not code contribution facts".into()));
            }
            if self.data.get("author").and_then(Value::as_str).unwrap_or("").trim().is_empty()
                || self.data.get("authoredAt").and_then(Value::as_str).unwrap_or("").is_empty()
            {
                return Err(Error::Invalid(
                    "commit requires its actual author and authoredAt".into(),
                ));
            }
            for key in ["additions", "deletions"] {
                if !self.data.get(key).is_none_or(|v| v.is_null() || v.as_u64().is_some()) {
                    return Err(Error::Invalid(format!("invalid {key}")));
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NewSource {
    pub project_id: String,
    pub cloud_org_id: String,
    pub cloud_project_id: String,
    pub name: String,
}
impl NewSource {
    pub fn validate(&self) -> Result<(), Error> {
        if [&self.project_id, &self.cloud_org_id, &self.cloud_project_id, &self.name]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 255)
        {
            return Err(Error::Invalid("source fields are required (max 255 characters)".into()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CompleteRun {
    pub counts: std::collections::BTreeMap<String, i64>,
    pub code_since: String,
    pub code_until: String,
}
impl CompleteRun {
    pub fn validate(&self) -> Result<(), Error> {
        if !(self.counts.len() == 4 || self.counts.len() == 5)
            || self.counts.keys().any(|k| !KINDS.contains(&k.as_str()))
            || ["requirement", "bug", "sprint", "commit"]
                .iter()
                .any(|k| self.counts.get(*k).is_none_or(|n| *n < 0))
            || self.counts.get("testcase").is_some_and(|n| *n < 0)
        {
            return Err(Error::Invalid(
                "expected counts for all core record kinds (testcase optional)".into(),
            ));
        }
        let a = chrono::DateTime::parse_from_rfc3339(&self.code_since)
            .map_err(|_| Error::Invalid("invalid codeSince".into()))?;
        let b = chrono::DateTime::parse_from_rfc3339(&self.code_until)
            .map_err(|_| Error::Invalid("invalid codeUntil".into()))?;
        if a >= b {
            return Err(Error::Invalid("invalid code coverage range".into()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Default)]
#[cfg_attr(feature = "http", derive(utoipa::IntoParams))]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub kind: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub days: Option<i64>,
    pub group: Option<String>,
    pub delayed_only: Option<bool>,
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn refuses_merges_and_missing_authors() {
        let mut r = Record {
            kind: "commit".into(),
            external_id: "repo:sha".into(),
            data: json!({"author":"研发","authoredAt":"2026-10-28T10:00:00+08:00","parentIds":["a","b"]}),
        };
        assert!(r.validate().is_err());
        r.data["parentIds"] = json!(["a"]);
        assert!(r.validate().is_ok());
        r.data["author"] = json!("");
        assert!(r.validate().is_err());
    }
    #[test]
    fn validates_dates_and_complete_counts() {
        let r = Record {
            kind: "requirement".into(),
            external_id: "1".into(),
            data: json!({"due":"2026-02-30"}),
        };
        assert!(r.validate().is_err());
        let c = CompleteRun {
            counts: Default::default(),
            code_since: "a".into(),
            code_until: "b".into(),
        };
        assert!(c.validate().is_err());
    }
}

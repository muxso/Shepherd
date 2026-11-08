use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("{0}")]
    Invalid(String),
    #[error("permission denied")]
    Forbidden,
    #[error("asset not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("asset storage unavailable")]
    Storage,
}
pub type AssetResult<T> = Result<T, AssetError>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryBinding {
    pub id: String,
    pub name: String,
    pub branch: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub commit_sha: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub repositories: Vec<RepositoryBinding>,
}
impl ServiceInput {
    pub fn validate(&self) -> AssetResult<()> {
        bounded(&self.name, 150, true)?;
        bounded(&self.description, 8000, false)?;
        if self.repositories.len() > 100 {
            return Err(AssetError::Invalid("at most 100 repositories".into()));
        }
        let mut ids = std::collections::BTreeSet::new();
        for r in &self.repositories {
            bounded(&r.id, 255, true)?;
            bounded(&r.name, 500, true)?;
            bounded(&r.branch, 255, true)?;
            if !ids.insert(&r.id) || (!r.url.is_empty() && !r.url.starts_with("https://")) {
                return Err(AssetError::Invalid(
                    "unique repositories and HTTPS source URLs required".into(),
                ));
            }
            if !r.commit_sha.is_empty() && !valid_sha(&r.commit_sha) {
                return Err(AssetError::Invalid("invalid commit SHA".into()));
            }
        }
        Ok(())
    }
}
pub fn valid_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|c| c.is_ascii_hexdigit())
}
pub fn bounded(value: &str, max: usize, required: bool) -> AssetResult<()> {
    if (required && value.trim().is_empty())
        || value.len() > max
        || value.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
    {
        Err(AssetError::Invalid("invalid or oversized field".into()))
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioStep {
    pub case_id: String,
    #[serde(default)]
    pub instruction: String,
    pub expected: String,
    #[serde(default)]
    pub parameters: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub preconditions: String,
    pub steps: Vec<ScenarioStep>,
    #[serde(default)]
    pub requirement_ids: Vec<String>,
    pub api_scenario_id: Option<String>,
    #[serde(default = "draft")]
    pub status: String,
}
fn draft() -> String {
    "DRAFT".into()
}
impl ScenarioInput {
    pub fn validate(&self) -> AssetResult<()> {
        bounded(&self.name, 250, true)?;
        bounded(&self.description, 16000, false)?;
        bounded(&self.preconditions, 8000, false)?;
        if self.steps.is_empty()
            || self.steps.len() > 200
            || self.requirement_ids.len() > 100
            || !matches!(self.status.as_str(), "DRAFT" | "REVIEWED")
        {
            return Err(AssetError::Invalid(
                "scenario requires 1..200 steps and a valid status".into(),
            ));
        }
        for s in &self.steps {
            bounded(&s.case_id, 255, true)?;
            bounded(&s.expected, 8000, true)?;
            bounded(&s.instruction, 8000, false)?;
            bounded(&s.parameters, 8000, false)?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub source_id: String,
    pub revision: String,
    pub records: Vec<SourceRecord>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRecord {
    pub kind: String,
    pub external_id: String,
    pub data: Value,
}

use super::Error;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Analysis {
    pub scope: Scope,
    pub task_id: String,
    pub project_key: String,
    pub repository_id: String,
    pub pull_request: Option<String>,
    pub revision: String,
    pub author: Option<String>,
    pub analyzed_at: DateTime<Utc>,
    pub gate: Gate,
    pub new_issues: Option<u32>,
    pub new_coverage: Option<f64>,
    pub new_duplication: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push: Option<Push>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_code: Option<NewCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_quality: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Push {
    pub base_revision: String,
    pub branch: String,
    pub pushed_at: DateTime<Utc>,
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_verified: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewCode {
    pub lines: Option<u32>,
    pub security_issues: Option<u32>,
    pub security_rating: Option<u8>,
    pub reliability_issues: Option<u32>,
    pub reliability_rating: Option<u8>,
    pub maintainability_issues: Option<u32>,
    pub maintainability_rating: Option<u8>,
    pub accepted_issues: Option<u32>,
    pub security_hotspots: Option<u32>,
    pub lines_to_cover: Option<u32>,
    pub duplicated_lines: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    SonarRepository,
    CiPullRequest,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SonarRepository => "sonar_repository",
            Self::CiPullRequest => "ci_pull_request",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Gate {
    Ok,
    Error,
    Warn,
    None,
}

impl Analysis {
    pub fn validate(&self) -> Result<(), Error> {
        let identifiers = [&self.task_id, &self.project_key, &self.repository_id];
        if identifiers.into_iter().any(|value| !valid_id(value)) {
            return Err(Error::Invalid("invalid quality report metadata".into()));
        }
        match (self.scope, self.pull_request.as_deref(), self.author.as_deref()) {
            (Scope::SonarRepository, None, None) => {}
            (Scope::CiPullRequest, Some(pr), Some(author)) if valid_id(pr) && valid_id(author) => {}
            _ => {
                return Err(Error::Invalid(
                    "PR reports require a PR and author; repository reports omit both".into(),
                ))
            }
        }
        if !valid_sha(&self.revision) {
            return Err(Error::Invalid("invalid commit SHA".into()));
        }
        if let Some(push) = &self.push {
            if !matches!(self.scope, Scope::SonarRepository)
                || !valid_sha(&push.base_revision)
                || self.revision.eq_ignore_ascii_case(&push.base_revision)
                || !valid_id(&push.branch)
                || push.author.as_deref().is_some_and(|author| !valid_id(author))
            {
                return Err(Error::Invalid("invalid push baseline or authorship".into()));
            }
        }
        if let Some(code) = &self.new_code {
            if [code.security_rating, code.reliability_rating, code.maintainability_rating]
                .into_iter()
                .flatten()
                .any(|value| !(1..=5).contains(&value))
            {
                return Err(Error::Invalid("invalid Sonar rating".into()));
            }
        }
        if [self.new_coverage, self.new_duplication]
            .into_iter()
            .flatten()
            .any(|value| !(0.0..=100.0).contains(&value))
        {
            return Err(Error::Invalid("invalid quality metric percentage".into()));
        }
        Ok(())
    }
}

fn valid_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().any(|byte| byte != b'0')
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_metadata_for_each_report_scope() {
        let mut body = json!({
            "scope": "sonar_repository",
            "taskId": "task",
            "projectKey": "project",
            "repositoryId": "repo",
            "revision": "a".repeat(40),
            "analyzedAt": "2026-11-20T00:00:00Z",
            "gate": "OK"
        });
        let validate = |body: &serde_json::Value| {
            serde_json::from_value::<Analysis>(body.clone()).unwrap().validate()
        };
        assert!(validate(&body).is_ok());
        body["scope"] = json!("ci_pull_request");
        assert!(validate(&body).is_err());
        body["pullRequest"] = json!("42");
        body["author"] = json!("codeup-user");
        assert!(validate(&body).is_ok());
        body["scope"] = json!("sonar_repository");
        assert!(validate(&body).is_err());
    }
}

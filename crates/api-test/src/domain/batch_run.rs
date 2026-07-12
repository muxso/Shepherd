#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchRunMode {
    Parallel,
    Serial,
}

impl BatchRunMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            BatchRunMode::Parallel => "PARALLEL",
            BatchRunMode::Serial => "SERIAL",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "PARALLEL" => Some(BatchRunMode::Parallel),
            "SERIAL" => Some(BatchRunMode::Serial),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryConfig {
    pub times: u32,
    pub interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunModeConfig {
    pub mode: BatchRunMode,
    pub pool_id: Option<String>,
    pub retry: Option<RetryConfig>,
    pub environment_id: Option<String>,
}

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BatchRunError {
    #[error("no cases to run")]
    NoCases,
    #[error("invalid retry config")]
    InvalidRetryConfig,
    #[error("resource pool not configured")]
    ResourcePoolNotConfigured,
    #[error("resource pool unavailable: {pool_id}")]
    ResourcePoolUnavailable { pool_id: String },
    #[error("backend error: {0}")]
    Backend(String),
}

fn non_blank(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

pub fn resolve_effective_pool(
    client_pool: Option<&str>,
    project_default_pool: Option<&str>,
) -> Result<String, BatchRunError> {
    non_blank(client_pool)
        .or_else(|| non_blank(project_default_pool))
        .ok_or(BatchRunError::ResourcePoolNotConfigured)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRunCommand {
    pub case_ids: Vec<String>,
    pub config: RunModeConfig,
}

impl BatchRunCommand {
    pub fn new(case_ids: Vec<String>, config: RunModeConfig) -> Result<Self, BatchRunError> {
        if case_ids.is_empty() {
            return Err(BatchRunError::NoCases);
        }
        if let Some(retry) = &config.retry {
            if retry.times == 0 {
                return Err(BatchRunError::InvalidRetryConfig);
            }
        }
        Ok(Self { case_ids, config })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pool: Option<&str>) -> RunModeConfig {
        RunModeConfig {
            mode: BatchRunMode::Parallel,
            pool_id: pool.map(str::to_string),
            retry: None,
            environment_id: None,
        }
    }

    #[test]
    fn mode_string_roundtrip() {
        assert_eq!(BatchRunMode::parse("parallel"), Some(BatchRunMode::Parallel));
        assert_eq!(BatchRunMode::parse("SERIAL"), Some(BatchRunMode::Serial));
        assert_eq!(BatchRunMode::parse("x"), None);
    }

    #[test]
    fn client_pool_takes_precedence() {
        assert_eq!(
            resolve_effective_pool(Some("client-pool"), Some("proj-pool")),
            Ok("client-pool".into())
        );
    }

    #[test]
    fn falls_back_to_project_default_when_client_blank() {
        assert_eq!(resolve_effective_pool(None, Some("proj-pool")), Ok("proj-pool".into()));
        assert_eq!(resolve_effective_pool(Some("   "), Some("proj-pool")), Ok("proj-pool".into()));
    }

    #[test]
    fn neither_pool_configured_is_explicit_error() {
        assert_eq!(
            resolve_effective_pool(None, None),
            Err(BatchRunError::ResourcePoolNotConfigured)
        );
        assert_eq!(
            resolve_effective_pool(Some(" "), Some("")),
            Err(BatchRunError::ResourcePoolNotConfigured)
        );
    }

    #[test]
    fn empty_cases_rejected() {
        assert_eq!(BatchRunCommand::new(vec![], cfg(Some("p"))), Err(BatchRunError::NoCases));
    }

    #[test]
    fn non_empty_cases_ok() {
        assert!(BatchRunCommand::new(vec!["c1".into()], cfg(Some("p"))).is_ok());
    }

    #[test]
    fn retry_enabled_with_zero_times_rejected() {
        let config = RunModeConfig {
            mode: BatchRunMode::Serial,
            pool_id: Some("p".into()),
            retry: Some(RetryConfig { times: 0, interval_ms: 100 }),
            environment_id: None,
        };
        assert_eq!(
            BatchRunCommand::new(vec!["c1".into()], config),
            Err(BatchRunError::InvalidRetryConfig)
        );
    }
}

//! 批量运行的领域模型 + 资源池解析规则。纯逻辑。

/// 批量运行模式(对应 ApiBatchRunMode)。
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

/// 失败重试配置(对应 ApiRunRetryConfig)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryConfig {
    pub times: u32,
    pub interval_ms: u64,
}

/// 运行模式配置(对应 ApiRunModeConfigDTO 的核心字段)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunModeConfig {
    pub mode: BatchRunMode,
    /// 客户端指定的资源池;`None`/空表示交由项目默认池解析。
    pub pool_id: Option<String>,
    /// 失败重试:开启则必须带合法的重试次数。
    pub retry: Option<RetryConfig>,
}

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BatchRunError {
    /// 没有任何用例可运行。
    #[error("no cases to run")]
    NoCases,
    /// 开启了重试但次数 <= 0。
    #[error("invalid retry config")]
    InvalidRetryConfig,
    /// 客户端与项目默认池都解析不出资源池(Java 版此时深在执行链里 500)。
    #[error("resource pool not configured")]
    ResourcePoolNotConfigured,
    /// 解析出了池,但不可用(不存在/被禁用/无在线节点)。
    #[error("resource pool unavailable: {pool_id}")]
    ResourcePoolUnavailable { pool_id: String },
    /// 下游基础设施错误。
    #[error("backend error: {0}")]
    Backend(String),
}

/// 把空白字符串规整为 `None`。
fn non_blank(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

/// 解析有效资源池:**客户端 poolId 优先,否则回退项目默认池**;两者皆空则报错。
///
/// 这是整条 quirk 的核心:把"必须有可解析的池"前移到入口,显式失败。
pub fn resolve_effective_pool(
    client_pool: Option<&str>,
    project_default_pool: Option<&str>,
) -> Result<String, BatchRunError> {
    non_blank(client_pool)
        .or_else(|| non_blank(project_default_pool))
        .ok_or(BatchRunError::ResourcePoolNotConfigured)
}

/// 已校验的批量运行命令。
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
        }
    }

    // ---- 模式解析 ----
    #[test]
    fn mode_string_roundtrip() {
        assert_eq!(BatchRunMode::parse("parallel"), Some(BatchRunMode::Parallel));
        assert_eq!(BatchRunMode::parse("SERIAL"), Some(BatchRunMode::Serial));
        assert_eq!(BatchRunMode::parse("x"), None);
    }

    // ---- 资源池解析(quirk 核心) ----
    #[test]
    fn client_pool_takes_precedence() {
        assert_eq!(resolve_effective_pool(Some("client-pool"), Some("proj-pool")), Ok("client-pool".into()));
    }

    #[test]
    fn falls_back_to_project_default_when_client_blank() {
        assert_eq!(resolve_effective_pool(None, Some("proj-pool")), Ok("proj-pool".into()));
        assert_eq!(resolve_effective_pool(Some("   "), Some("proj-pool")), Ok("proj-pool".into()));
    }

    #[test]
    fn neither_pool_configured_is_explicit_error() {
        // 这正是 Java 版会 500 的场景,这里在入口就明确失败
        assert_eq!(resolve_effective_pool(None, None), Err(BatchRunError::ResourcePoolNotConfigured));
        assert_eq!(
            resolve_effective_pool(Some(" "), Some("")),
            Err(BatchRunError::ResourcePoolNotConfigured)
        );
    }

    // ---- 命令校验 ----
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
        };
        assert_eq!(
            BatchRunCommand::new(vec!["c1".into()], config),
            Err(BatchRunError::InvalidRetryConfig)
        );
    }
}

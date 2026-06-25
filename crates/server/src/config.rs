//! 服务端**启动期**配置:把原先散落在 `main` 各处的 `env::var` 收拢成单一 typed 真源。
//!
//! 收益:所有启动开关在一处声明(自文档化)、解析/默认值一次定型、可单测(解析逻辑经
//! `resolve(lookup)` 注入查找闭包,不碰真实进程环境、无并发竞态)。
//!
//! 范围:**仅**覆盖启动时一次性读取的配置。特性内按需懒读的 env(`llm` / `planner` / `judge` /
//! `orchestration` / `report_archive_job` / `perf_run`)仍留在各自模块,避免把无关耦合拉进装配根。

/// OIDC 第三方登录 provider 凭证(飞书 / 企业微信)。仅当 id 与 secret **都**配置时才构造,
/// 对应「未配置则无该 provider、端点 404」的原语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcProviderConfig {
    pub app_id: String,
    pub app_secret: String,
    /// 回调地址;未配置时为空串(沿用原 `unwrap_or_default`)。
    pub redirect: String,
}

/// agent 执行者 / 机群相关开关。优先级级联(FLEET > URL > CMD > LLM > Echo)仍在 `main` 里,
/// 这里只持有其判定所需的原始 typed 值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfig {
    /// `SHEPHERD_AGENT_FLEET` 是否存在 → 机群模式(派发入队 + 远程 runtime 出站认领)。
    pub fleet: bool,
    /// `SHEPHERD_FLEET_REDIS`:分布式队列/注册表的 Redis URL;None=进程内(单机)。
    pub fleet_redis: Option<String>,
    /// 回收死 runtime 的 pending 的轮询间隔秒(默认 15)。
    pub fleet_reap_interval_s: u64,
    /// pending 持有者离线后判定可回收的容忍期毫秒(默认 30000)。
    pub fleet_reclaim_ms: u64,
    /// `SHEPHERD_AGENT_URL`:HTTP 执行者地址。
    pub url: Option<String>,
    /// `SHEPHERD_AGENT_CMD`:本地命令执行者 argv(原样字符串,空白分割在 `main`)。
    pub cmd: Option<String>,
    /// `SHEPHERD_AGENT_ASYNC` 是否存在 → 命令执行者异步回调模式。
    pub async_callback: bool,
}

/// 服务端启动配置。`from_env()` 一次性读取;字段语义/默认值见各处文档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// `DATABASE_URL`:PG 连接串(默认本地 mstest)。
    pub db_url: String,
    /// `SHEPHERD_BIND`:主 API 监听地址(默认 `0.0.0.0:8088`,与 shepherd CLI 默认端口对齐)。
    pub bind: String,
    /// `MOCK_BIND`:Mock 服务独立监听;仅在配置且非空白时启用。
    pub mock_bind: Option<String>,
    /// `SHEPHERD_ADMIN_PASSWORD`:启动时幂等 upsert 的 admin 密码(默认 `admin`)。
    pub admin_pw: String,
    /// `SHEPHERD_SESSION_TTL_SECS`:会话有效期秒(默认 8 小时)。
    pub session_ttl_secs: i64,
    /// 飞书 OIDC(`SHEPHERD_FEISHU_APP_ID` / `_APP_SECRET` / `_REDIRECT`)。
    pub feishu: Option<OidcProviderConfig>,
    /// 企业微信 OIDC(`SHEPHERD_WECOM_CORP_ID` / `_SECRET` / `_REDIRECT`)。
    pub wecom: Option<OidcProviderConfig>,
    /// agent 执行者 / 机群开关。
    pub agent: AgentConfig,
    /// `SHEPHERD_EXECUTOR_URL`:api-test 远端 JMeter 派发地址;仅在配置且非空时启用。
    pub executor_url: Option<String>,
    /// `SHEPHERD_RUNNER == "noop"`:显式声明本地无执行器(批量运行恒 RUNNING,仅供演示)。
    pub runner_noop: bool,
}

impl ServerConfig {
    /// 从进程环境读取。
    pub fn from_env() -> Self {
        Self::resolve(|key| std::env::var(key).ok())
    }

    /// 用注入的查找闭包解析(供单测,不碰真实 env)。`lookup(key)` 返回 `None` 即「未设置」;
    /// 返回 `Some("")` 视为「已设置为空」(与 `env::var(k).is_ok()` 一致)。
    pub fn resolve(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let present = |key: &str| lookup(key).is_some();
        let parse_or = |key: &str, default: i64| -> i64 {
            lookup(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        };
        let parse_u64 = |key: &str, default: u64| -> u64 {
            lookup(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        };
        let oidc = |id_key: &str, secret_key: &str, redirect_key: &str| {
            match (lookup(id_key), lookup(secret_key)) {
                (Some(app_id), Some(app_secret)) => Some(OidcProviderConfig {
                    app_id,
                    app_secret,
                    redirect: lookup(redirect_key).unwrap_or_default(),
                }),
                _ => None,
            }
        };

        ServerConfig {
            db_url: lookup("DATABASE_URL")
                .unwrap_or_else(|| "postgres://msuser:mspass@localhost:55432/mstest".to_string()),
            bind: lookup("SHEPHERD_BIND").unwrap_or_else(|| "0.0.0.0:8088".to_string()),
            mock_bind: lookup("MOCK_BIND").filter(|v| !v.trim().is_empty()),
            admin_pw: lookup("SHEPHERD_ADMIN_PASSWORD").unwrap_or_else(|| "admin".to_string()),
            session_ttl_secs: parse_or("SHEPHERD_SESSION_TTL_SECS", 8 * 3600),
            feishu: oidc("SHEPHERD_FEISHU_APP_ID", "SHEPHERD_FEISHU_APP_SECRET", "SHEPHERD_FEISHU_REDIRECT"),
            wecom: oidc("SHEPHERD_WECOM_CORP_ID", "SHEPHERD_WECOM_SECRET", "SHEPHERD_WECOM_REDIRECT"),
            agent: AgentConfig {
                fleet: present("SHEPHERD_AGENT_FLEET"),
                fleet_redis: lookup("SHEPHERD_FLEET_REDIS"),
                fleet_reap_interval_s: parse_u64("SHEPHERD_FLEET_REAP_INTERVAL_S", 15),
                fleet_reclaim_ms: parse_u64("SHEPHERD_FLEET_RECLAIM_MS", 30_000),
                url: lookup("SHEPHERD_AGENT_URL"),
                cmd: lookup("SHEPHERD_AGENT_CMD"),
                async_callback: present("SHEPHERD_AGENT_ASYNC"),
            },
            executor_url: lookup("SHEPHERD_EXECUTOR_URL").filter(|v| !v.is_empty()),
            runner_noop: lookup("SHEPHERD_RUNNER").as_deref() == Some("noop"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn from(pairs: &[(&str, &str)]) -> ServerConfig {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        ServerConfig::resolve(move |k| map.get(k).cloned())
    }

    #[test]
    fn defaults_when_unset() {
        let cfg = from(&[]);
        assert_eq!(cfg.bind, "0.0.0.0:8088");
        assert_eq!(cfg.admin_pw, "admin");
        assert_eq!(cfg.session_ttl_secs, 8 * 3600);
        assert_eq!(cfg.mock_bind, None);
        assert_eq!(cfg.feishu, None);
        assert_eq!(cfg.wecom, None);
        assert!(!cfg.agent.fleet);
        assert_eq!(cfg.agent.fleet_reap_interval_s, 15);
        assert_eq!(cfg.agent.fleet_reclaim_ms, 30_000);
        assert_eq!(cfg.executor_url, None);
        assert!(!cfg.runner_noop);
    }

    #[test]
    fn blank_values_are_treated_as_unset_where_it_matters() {
        // MOCK_BIND / SHEPHERD_EXECUTOR_URL 空白视为未启用(沿用原逻辑)。
        let cfg = from(&[("MOCK_BIND", "  "), ("SHEPHERD_EXECUTOR_URL", "")]);
        assert_eq!(cfg.mock_bind, None);
        assert_eq!(cfg.executor_url, None);
    }

    #[test]
    fn oidc_needs_both_id_and_secret() {
        // 只配 id、缺 secret → 不构造 provider。
        let cfg = from(&[("SHEPHERD_FEISHU_APP_ID", "x")]);
        assert_eq!(cfg.feishu, None);
        // id + secret 齐 → 构造;redirect 缺省空串。
        let cfg = from(&[("SHEPHERD_FEISHU_APP_ID", "x"), ("SHEPHERD_FEISHU_APP_SECRET", "y")]);
        assert_eq!(
            cfg.feishu,
            Some(OidcProviderConfig { app_id: "x".into(), app_secret: "y".into(), redirect: String::new() })
        );
    }

    #[test]
    fn presence_flags_and_parsing() {
        let cfg = from(&[
            ("SHEPHERD_AGENT_FLEET", ""), // 仅存在即生效
            ("SHEPHERD_FLEET_REDIS", "redis://h:6379"),
            ("SHEPHERD_FLEET_REAP_INTERVAL_S", "5"),
            ("SHEPHERD_FLEET_RECLAIM_MS", "9000"),
            ("SHEPHERD_RUNNER", "noop"),
            ("SHEPHERD_SESSION_TTL_SECS", "abc"), // 解析失败 → 回落默认
        ]);
        assert!(cfg.agent.fleet);
        assert_eq!(cfg.agent.fleet_redis.as_deref(), Some("redis://h:6379"));
        assert_eq!(cfg.agent.fleet_reap_interval_s, 5);
        assert_eq!(cfg.agent.fleet_reclaim_ms, 9000);
        assert!(cfg.runner_noop);
        assert_eq!(cfg.session_ttl_secs, 8 * 3600);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcProviderConfig {
    pub app_id: String,
    pub app_secret: String,
    pub redirect: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfig {
    pub fleet: bool,
    pub fleet_redis: Option<String>,
    pub fleet_reap_interval_s: u64,
    pub fleet_reclaim_ms: u64,
    pub url: Option<String>,
    pub cmd: Option<String>,
    pub async_callback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub db_url: String,
    pub bind: String,
    pub mock_bind: Option<String>,
    pub admin_pw: String,
    pub session_ttl_secs: i64,
    pub feishu: Option<OidcProviderConfig>,
    pub wecom: Option<OidcProviderConfig>,
    pub agent: AgentConfig,
    pub executor_url: Option<String>,
    pub runner_noop: bool,
}

/// Parses a `KEY=VALUE` env file: skips blank lines and `#` comments, trims keys
/// and values, allows an `export ` prefix, strips matching surrounding quotes.
/// No variable interpolation.
fn parse_env_file(contents: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(v);
        out.insert(k.to_string(), v.to_string());
    }
    out
}

/// Layered lookup: the process environment always wins, then file layers in
/// priority order. Existing env-var-only deployments behave unchanged.
fn layered_lookup(
    env: impl Fn(&str) -> Option<String>,
    files: Vec<std::collections::HashMap<String, String>>,
) -> impl Fn(&str) -> Option<String> {
    move |key| env(key).or_else(|| files.iter().find_map(|m| m.get(key).cloned()))
}

impl ServerConfig {
    /// Load order (earlier wins): process env → `shepherd.${SHEPHERD_ENV}.env`
    /// (profile) → `shepherd.env` (shared defaults). File directory comes from
    /// `SHEPHERD_CONFIG_DIR`, defaulting to the current directory.
    pub fn from_env() -> Self {
        let dir = std::env::var("SHEPHERD_CONFIG_DIR").unwrap_or_else(|_| ".".to_string());
        let read = |path: String| std::fs::read_to_string(path).ok().map(|c| parse_env_file(&c));
        let mut files = Vec::new();
        if let Ok(name) = std::env::var("SHEPHERD_ENV") {
            let name = name.trim();
            if !name.is_empty() {
                if let Some(m) = read(format!("{dir}/shepherd.{name}.env")) {
                    files.push(m);
                }
            }
        }
        if let Some(m) = read(format!("{dir}/shepherd.env")) {
            files.push(m);
        }
        Self::resolve(layered_lookup(|key| std::env::var(key).ok(), files))
    }

    pub fn resolve(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let present = |key: &str| lookup(key).is_some();
        let parse_or = |key: &str, default: i64| -> i64 {
            lookup(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        };
        let parse_u64 = |key: &str, default: u64| -> u64 {
            lookup(key).and_then(|v| v.parse().ok()).unwrap_or(default)
        };
        let oidc = |id_key: &str, secret_key: &str, redirect_key: &str| match (
            lookup(id_key),
            lookup(secret_key),
        ) {
            (Some(app_id), Some(app_secret)) => Some(OidcProviderConfig {
                app_id,
                app_secret,
                redirect: lookup(redirect_key).unwrap_or_default(),
            }),
            _ => None,
        };

        ServerConfig {
            db_url: lookup("DATABASE_URL")
                .unwrap_or_else(|| "postgres://msuser:mspass@localhost:55432/mstest".to_string()),
            bind: lookup("SHEPHERD_BIND").unwrap_or_else(|| "0.0.0.0:8088".to_string()),
            mock_bind: lookup("MOCK_BIND").filter(|v| !v.trim().is_empty()),
            admin_pw: lookup("SHEPHERD_ADMIN_PASSWORD").unwrap_or_else(|| "admin".to_string()),
            session_ttl_secs: parse_or("SHEPHERD_SESSION_TTL_SECS", 8 * 3600),
            feishu: oidc(
                "SHEPHERD_FEISHU_APP_ID",
                "SHEPHERD_FEISHU_APP_SECRET",
                "SHEPHERD_FEISHU_REDIRECT",
            ),
            wecom: oidc(
                "SHEPHERD_WECOM_CORP_ID",
                "SHEPHERD_WECOM_SECRET",
                "SHEPHERD_WECOM_REDIRECT",
            ),
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
        let cfg = from(&[("MOCK_BIND", "  "), ("SHEPHERD_EXECUTOR_URL", "")]);
        assert_eq!(cfg.mock_bind, None);
        assert_eq!(cfg.executor_url, None);
    }

    #[test]
    fn oidc_needs_both_id_and_secret() {
        let cfg = from(&[("SHEPHERD_FEISHU_APP_ID", "x")]);
        assert_eq!(cfg.feishu, None);
        let cfg = from(&[("SHEPHERD_FEISHU_APP_ID", "x"), ("SHEPHERD_FEISHU_APP_SECRET", "y")]);
        assert_eq!(
            cfg.feishu,
            Some(OidcProviderConfig {
                app_id: "x".into(),
                app_secret: "y".into(),
                redirect: String::new()
            })
        );
    }

    #[test]
    fn env_file_parsing() {
        let m = parse_env_file(
            "# comment\n\nSHEPHERD_BIND=127.0.0.1:9180\nexport SHEPHERD_ENV=prod\nQUOTED=\"a b\"\nSQ='x'\nDATABASE_URL=postgres://u:p@h/db?x=1\n=skip\n  PAD = 3 \n",
        );
        assert_eq!(m.get("SHEPHERD_BIND").unwrap(), "127.0.0.1:9180");
        assert_eq!(m.get("SHEPHERD_ENV").unwrap(), "prod"); // export prefix
        assert_eq!(m.get("QUOTED").unwrap(), "a b"); // quotes stripped
        assert_eq!(m.get("SQ").unwrap(), "x");
        assert_eq!(m.get("DATABASE_URL").unwrap(), "postgres://u:p@h/db?x=1"); // '=' kept inside value
        assert_eq!(m.get("PAD").unwrap(), "3"); // key and value trimmed
        assert!(!m.contains_key("")); // empty key skipped
    }

    #[test]
    fn process_env_wins_over_files_then_profile_over_base() {
        let mut base = HashMap::new();
        base.insert("SHEPHERD_BIND".to_string(), "base:1".to_string());
        base.insert("SHEPHERD_ADMIN_PASSWORD".to_string(), "frombase".to_string());
        let mut profile = HashMap::new();
        profile.insert("SHEPHERD_BIND".to_string(), "profile:2".to_string());
        // Priority: process env > profile > base.
        let env: HashMap<String, String> =
            [("SHEPHERD_BIND".to_string(), "env:3".to_string())].into_iter().collect();
        let lookup = layered_lookup(move |k| env.get(k).cloned(), vec![profile, base]);
        let cfg = ServerConfig::resolve(lookup);
        assert_eq!(cfg.bind, "env:3"); // process env wins
        assert_eq!(cfg.admin_pw, "frombase"); // only base provides it -> falls back to base
    }

    #[test]
    fn profile_layer_overrides_base_when_env_absent() {
        let base: HashMap<String, String> =
            [("SHEPHERD_BIND".to_string(), "base:1".to_string())].into_iter().collect();
        let profile: HashMap<String, String> =
            [("SHEPHERD_BIND".to_string(), "profile:2".to_string())].into_iter().collect();
        let lookup = layered_lookup(|_| None, vec![profile, base]);
        assert_eq!(ServerConfig::resolve(lookup).bind, "profile:2");
    }

    #[test]
    fn presence_flags_and_parsing() {
        let cfg = from(&[
            ("SHEPHERD_AGENT_FLEET", ""),
            ("SHEPHERD_FLEET_REDIS", "redis://h:6379"),
            ("SHEPHERD_FLEET_REAP_INTERVAL_S", "5"),
            ("SHEPHERD_FLEET_RECLAIM_MS", "9000"),
            ("SHEPHERD_RUNNER", "noop"),
            ("SHEPHERD_SESSION_TTL_SECS", "abc"),
        ]);
        assert!(cfg.agent.fleet);
        assert_eq!(cfg.agent.fleet_redis.as_deref(), Some("redis://h:6379"));
        assert_eq!(cfg.agent.fleet_reap_interval_s, 5);
        assert_eq!(cfg.agent.fleet_reclaim_ms, 9000);
        assert!(cfg.runner_noop);
        assert_eq!(cfg.session_ttl_secs, 8 * 3600);
    }
}

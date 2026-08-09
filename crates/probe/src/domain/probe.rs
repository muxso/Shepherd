use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeRequest {
    pub protocol: String,
    pub target: String,
    #[serde(default)]
    pub payload: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub assertions: Vec<ProbeAssertion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawProbe {
    pub transport_ok: bool,
    pub status: Option<i64>,
    pub latency_ms: u64,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ProbeAssertion {
    Success,
    StatusIs(i64),
    OutputContains(String),
    OutputEquals(String),
    LatencyUnderMs(u64),
}

impl ProbeAssertion {
    fn check(&self, raw: &RawProbe) -> Option<String> {
        match self {
            Self::Success => {
                if raw.transport_ok {
                    None
                } else {
                    Some(format!("transport failed: {}", raw.error.as_deref().unwrap_or("unknown")))
                }
            }
            Self::StatusIs(want) => match raw.status {
                Some(got) if got == *want => None,
                Some(got) => Some(format!("status: expected {want}, actual {got}")),
                None => Some(format!("status: expected {want}, no status code")),
            },
            Self::OutputContains(sub) => {
                let out = raw.output.as_deref().unwrap_or("");
                if out.contains(sub.as_str()) {
                    None
                } else {
                    Some(format!("output does not contain substring: {sub}"))
                }
            }
            Self::OutputEquals(want) => {
                let out = raw.output.as_deref().unwrap_or("");
                if out == want {
                    None
                } else {
                    Some("output does not equal the expected value".to_string())
                }
            }
            Self::LatencyUnderMs(max) => {
                if raw.latency_ms <= *max {
                    None
                } else {
                    Some(format!("latency {}ms exceeds {max}ms", raw.latency_ms))
                }
            }
        }
    }
}

pub fn evaluate(assertions: &[ProbeAssertion], raw: &RawProbe) -> Vec<String> {
    assertions.iter().filter_map(|a| a.check(raw)).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeOutcome {
    pub success: bool,
    pub status: Option<i64>,
    pub latency_ms: u64,
    pub output: Option<String>,
    pub failures: Vec<String>,
}

impl ProbeOutcome {
    /// On transport failure, fold the transport error into failures even without an
    /// explicit `Success` assertion (no silent failures).
    pub fn from_raw(raw: RawProbe, assertions: &[ProbeAssertion]) -> Self {
        let mut failures = evaluate(assertions, &raw);
        if !raw.transport_ok && !assertions.iter().any(|a| matches!(a, ProbeAssertion::Success)) {
            failures
                .push(format!("transport failed: {}", raw.error.as_deref().unwrap_or("unknown")));
        }
        Self {
            success: raw.transport_ok && failures.is_empty(),
            status: raw.status,
            latency_ms: raw.latency_ms,
            output: raw.output,
            failures,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_raw() -> RawProbe {
        RawProbe {
            transport_ok: true,
            status: Some(200),
            latency_ms: 12,
            output: Some(r#"{"name":"alice"}"#.into()),
            error: None,
        }
    }

    #[test]
    fn all_assertions_pass() {
        let a = vec![
            ProbeAssertion::Success,
            ProbeAssertion::StatusIs(200),
            ProbeAssertion::OutputContains("alice".into()),
            ProbeAssertion::LatencyUnderMs(100),
        ];
        assert!(evaluate(&a, &ok_raw()).is_empty());
        assert!(ProbeOutcome::from_raw(ok_raw(), &a).success);
    }

    #[test]
    fn status_and_latency_failures() {
        let a = vec![ProbeAssertion::StatusIs(500), ProbeAssertion::LatencyUnderMs(5)];
        let f = evaluate(&a, &ok_raw());
        assert_eq!(f.len(), 2);
        assert!(!ProbeOutcome::from_raw(ok_raw(), &a).success);
    }

    #[test]
    fn transport_failure_fails_success_assertion() {
        let raw =
            RawProbe { transport_ok: false, error: Some("refused".into()), ..Default::default() };
        let f = evaluate(&[ProbeAssertion::Success], &raw);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("transport failed"));
        assert!(!ProbeOutcome::from_raw(raw, &[]).success);
    }

    #[test]
    fn assertion_json_roundtrip() {
        let raw = r#"[{"type":"status_is","value":200},{"type":"output_contains","value":"ok"},{"type":"success"}]"#;
        let a: Vec<ProbeAssertion> = serde_json::from_str(raw).expect("parse");
        assert_eq!(a.len(), 3);
        assert_eq!(a[0], ProbeAssertion::StatusIs(200));
    }
}

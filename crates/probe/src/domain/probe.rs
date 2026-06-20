//! 协议无关的探测模型 + 通用断言(零 IO 纯函数)。
//!
//! 一次探测 = `ProbeRequest`(协议 + 目标 + 载荷 + 元数据 + 断言)。插件执行后产出 `RawProbe`
//! (传输是否成功 / 状态 / 延迟 / 输出),再由纯函数 `evaluate` 据断言判定,合成 `ProbeOutcome`。
//! 断言对**输入由调用方构造、输出由 evaluate 判定**,跨协议统一。

use serde::{Deserialize, Serialize};

/// 协议无关的探测请求(自包含,可逐次不同)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeRequest {
    /// 协议名(对应插件):http / grpc / sql / redis …
    pub protocol: String,
    /// 目标:HTTP URL / gRPC 端点 / SQL 连接串 …
    pub target: String,
    /// 载荷:HTTP body / gRPC 请求字节 / SQL 语句 …
    #[serde(default)]
    pub payload: Option<String>,
    /// 协议附加参数:HTTP method/headers、gRPC 方法路径 等。
    #[serde(default)]
    pub metadata: std::collections::BTreeMap<String, String>,
    /// 对输出的断言(全部满足才算通过)。
    #[serde(default)]
    pub assertions: Vec<ProbeAssertion>,
}

/// 插件执行后的原始结果(未判定断言)。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawProbe {
    /// 传输/连接是否成功(协议层面跑通,不含断言)。
    pub transport_ok: bool,
    /// 协议状态码(HTTP status / gRPC code …),无则 None。
    pub status: Option<i64>,
    pub latency_ms: u64,
    /// 输出文本(响应体 / 查询结果摘要 …)。
    pub output: Option<String>,
    /// 传输错误信息(transport_ok=false 时)。
    pub error: Option<String>,
}

/// 通用断言(跨协议)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ProbeAssertion {
    /// 传输成功(连接/调用未报错)。
    Success,
    /// 状态码等于。
    StatusIs(i64),
    /// 输出包含子串。
    OutputContains(String),
    /// 输出等于。
    OutputEquals(String),
    /// 延迟不超过 ms。
    LatencyUnderMs(u64),
}

impl ProbeAssertion {
    /// 对原始结果判定;不通过返回失败原因(Some),通过返回 None。
    fn check(&self, raw: &RawProbe) -> Option<String> {
        match self {
            Self::Success => {
                if raw.transport_ok {
                    None
                } else {
                    Some(format!("传输失败: {}", raw.error.as_deref().unwrap_or("unknown")))
                }
            }
            Self::StatusIs(want) => match raw.status {
                Some(got) if got == *want => None,
                Some(got) => Some(format!("status: 期望 {want},实际 {got}")),
                None => Some(format!("status: 期望 {want},无状态码")),
            },
            Self::OutputContains(sub) => {
                let out = raw.output.as_deref().unwrap_or("");
                if out.contains(sub.as_str()) {
                    None
                } else {
                    Some(format!("output 不含子串: {sub}"))
                }
            }
            Self::OutputEquals(want) => {
                let out = raw.output.as_deref().unwrap_or("");
                if out == want {
                    None
                } else {
                    Some("output 不等于期望值".to_string())
                }
            }
            Self::LatencyUnderMs(max) => {
                if raw.latency_ms <= *max {
                    None
                } else {
                    Some(format!("latency {}ms 超过 {max}ms", raw.latency_ms))
                }
            }
        }
    }
}

/// 纯函数:对原始结果逐条判定断言,收集失败原因。
pub fn evaluate(assertions: &[ProbeAssertion], raw: &RawProbe) -> Vec<String> {
    assertions.iter().filter_map(|a| a.check(raw)).collect()
}

/// 探测结果(判定后)。`success` = 传输成功且断言全过。
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
    /// 由原始结果 + 断言合成最终结果。
    /// 传输失败时,即使没写 `Success` 断言,也把传输错误并入 failures(避免静默失败)。
    pub fn from_raw(raw: RawProbe, assertions: &[ProbeAssertion]) -> Self {
        let mut failures = evaluate(assertions, &raw);
        if !raw.transport_ok && !assertions.iter().any(|a| matches!(a, ProbeAssertion::Success)) {
            failures.push(format!("传输失败: {}", raw.error.as_deref().unwrap_or("unknown")));
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
        let raw = RawProbe { transport_ok: false, error: Some("refused".into()), ..Default::default() };
        let f = evaluate(&[ProbeAssertion::Success], &raw);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("传输失败"));
        // 即使无断言,transport_ok=false → success=false
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

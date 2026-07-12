use async_trait::async_trait;

use api_runner::{HttpMethod, RequestSpec, ReqwestRunner};

use crate::domain::{ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

pub struct HttpPlugin {
    runner: ReqwestRunner,
}

impl Default for HttpPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpPlugin {
    pub fn new() -> Self {
        Self { runner: ReqwestRunner::no_proxy() }
    }
}

fn method_of(m: &str) -> HttpMethod {
    serde_json::from_value(serde_json::Value::String(m.to_uppercase())).unwrap_or(HttpMethod::Get)
}

#[async_trait]
impl ProtocolPlugin for HttpPlugin {
    fn protocol(&self) -> &'static str {
        "http"
    }

    async fn run(&self, req: &ProbeRequest) -> RawProbe {
        let method = req.metadata.get("method").map(|m| method_of(m)).unwrap_or(HttpMethod::Get);
        let spec = RequestSpec {
            method,
            url: req.target.clone(),
            headers: vec![],
            body: req.payload.clone(),
        };
        match self.runner.execute(&spec).await {
            Ok(snap) => RawProbe {
                transport_ok: true,
                status: Some(snap.status as i64),
                latency_ms: snap.elapsed_ms,
                output: Some(snap.body),
                error: None,
            },
            Err(e) => RawProbe {
                transport_ok: false,
                error: Some(format!("{}: {}", e.kind(), e.message())),
                ..Default::default()
            },
        }
    }
}

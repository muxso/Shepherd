use std::sync::Arc;

use async_trait::async_trait;

use probe::{PluginRegistry, ProbeRequest};

use crate::ports::RequestExecutor;

pub struct ProbeExecutor {
    registry: Arc<PluginRegistry>,
    request: ProbeRequest,
}

impl ProbeExecutor {
    pub fn new(registry: Arc<PluginRegistry>, request: ProbeRequest) -> Self {
        Self { registry, request }
    }
}

#[async_trait]
impl RequestExecutor for ProbeExecutor {
    async fn execute(&self) -> bool {
        self.registry.dispatch(&self.request).await.success
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use probe::{ProbeAssertion, ProtocolPlugin, RawProbe};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FlakyPlugin {
        calls: AtomicUsize,
        n: usize,
    }
    #[async_trait]
    impl ProtocolPlugin for FlakyPlugin {
        fn protocol(&self) -> &'static str {
            "flaky"
        }
        async fn run(&self, _req: &ProbeRequest) -> RawProbe {
            let i = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            let status = if i % self.n == 0 { 500 } else { 200 };
            RawProbe {
                transport_ok: true,
                status: Some(status),
                latency_ms: 1,
                output: Some("ok".into()),
                error: None,
            }
        }
    }

    fn req() -> ProbeRequest {
        ProbeRequest {
            protocol: "flaky".into(),
            target: "t".into(),
            payload: None,
            metadata: Default::default(),
            assertions: vec![ProbeAssertion::StatusIs(200)],
        }
    }

    #[tokio::test]
    async fn drives_plugin_and_applies_assertion() {
        let reg = Arc::new(
            PluginRegistry::new().with(Arc::new(FlakyPlugin { calls: AtomicUsize::new(0), n: 3 })),
        );
        let exec = ProbeExecutor::new(reg, req());
        assert!(exec.execute().await);
        assert!(exec.execute().await);
        assert!(!exec.execute().await);
    }

    #[tokio::test]
    async fn unsupported_protocol_is_failure() {
        let reg = Arc::new(PluginRegistry::new());
        let mut r = req();
        r.protocol = "nope".into();
        assert!(!ProbeExecutor::new(reg, r).execute().await);
    }
}

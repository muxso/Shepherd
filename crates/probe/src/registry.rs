use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::{ProbeOutcome, ProbeRequest, RawProbe};
use crate::ports::ProtocolPlugin;

#[derive(Default, Clone)]
pub struct PluginRegistry {
    plugins: HashMap<String, Arc<dyn ProtocolPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, plugin: Arc<dyn ProtocolPlugin>) -> Self {
        self.plugins.insert(plugin.protocol().to_string(), plugin);
        self
    }

    pub fn protocols(&self) -> Vec<String> {
        let mut v: Vec<String> = self.plugins.keys().cloned().collect();
        v.sort();
        v
    }

    pub async fn dispatch(&self, req: &ProbeRequest) -> ProbeOutcome {
        match self.plugins.get(&req.protocol) {
            Some(plugin) => {
                let raw = plugin.run(req).await;
                ProbeOutcome::from_raw(raw, &req.assertions)
            }
            None => ProbeOutcome::from_raw(
                RawProbe {
                    transport_ok: false,
                    error: Some(format!("unsupported protocol: {}", req.protocol)),
                    ..Default::default()
                },
                &req.assertions,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ProbeAssertion;
    use async_trait::async_trait;

    struct FakePlugin;
    #[async_trait]
    impl ProtocolPlugin for FakePlugin {
        fn protocol(&self) -> &'static str {
            "fake"
        }
        async fn run(&self, _req: &ProbeRequest) -> RawProbe {
            RawProbe { transport_ok: true, status: Some(200), latency_ms: 1, output: Some("pong".into()), error: None }
        }
    }

    fn req(protocol: &str, assertions: Vec<ProbeAssertion>) -> ProbeRequest {
        ProbeRequest {
            protocol: protocol.into(),
            target: "t".into(),
            payload: None,
            metadata: Default::default(),
            assertions,
        }
    }

    #[tokio::test]
    async fn dispatches_to_plugin_and_evaluates() {
        let reg = PluginRegistry::new().with(Arc::new(FakePlugin));
        assert_eq!(reg.protocols(), vec!["fake"]);
        let out = reg.dispatch(&req("fake", vec![ProbeAssertion::StatusIs(200), ProbeAssertion::OutputContains("pong".into())])).await;
        assert!(out.success);
        let bad = reg.dispatch(&req("fake", vec![ProbeAssertion::StatusIs(500)])).await;
        assert!(!bad.success);
        assert_eq!(bad.failures.len(), 1);
    }

    #[tokio::test]
    async fn unknown_protocol_fails() {
        let reg = PluginRegistry::new();
        let out = reg.dispatch(&req("nope", vec![])).await;
        assert!(!out.success);
        assert!(out.failures.iter().any(|f| f.contains("unsupported protocol")));
    }
}

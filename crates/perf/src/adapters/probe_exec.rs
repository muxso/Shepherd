//! 统一执行器:把压测引擎接到 `probe` 协议插件注册表。
//!
//! 一个执行器即可压测**任意**协议(http/grpc/sql/redis/…),由注册表按 `protocol` 分发,
//! 成功判定走 probe 的统一断言(`ProbeOutcome.success` = 传输成功且断言全过)。
//! 这样 perf 不必为每种协议各写一个 bespoke 执行器 —— 加协议 = 加插件,压测自动支持。
//!
//! 预置注册表 + 自包含 `ProbeRequest`;多个 worker 共享同一 `Arc<PluginRegistry>`,
//! 插件内部按 target 缓存连接(池/Channel),压测复用连接、不每请求重连。

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

    /// 假插件:每第 n 次调用返回非 200(用于验证断言判定 + 复用同一注册表)。
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
        // 第 1、2 次成功(200 命中断言),第 3 次失败(500)。
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

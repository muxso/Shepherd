//! CLI 执行后端抽象。claude/codex/opencode 各一实现(step 2);step 1 提供 Mock。
//!
//! 后端职责:spawn CLI 跑 prompt,边跑边经 sink 回流进度事件,返回**最终文本输出**。
//! git 快照 / 回调由 runtime 主流程按模式(implement / design)编排,后端不关心。

use async_trait::async_trait;
use thiserror::Error;

use crate::events::{ExecEvent, ProgressSink};

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend error: {0}")]
    Run(String),
}

#[async_trait]
pub trait CliAgentBackend: Send + Sync {
    /// 标识(claude/codex/opencode/mock)。
    fn cli_name(&self) -> &str;
    /// 跑 prompt,返回最终文本;运行中经 `sink` 回流事件。
    async fn execute(
        &self,
        prompt: &str,
        sink: &dyn ProgressSink,
    ) -> Result<String, BackendError>;
}

/// 测试 / 演示后端:不调真 CLI,回流一条事件并返回固定文本。
pub struct MockBackend {
    pub output: String,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self { output: "## Mock 输出\n(agent-runtime mock backend)".to_string() }
    }
}

#[async_trait]
impl CliAgentBackend for MockBackend {
    fn cli_name(&self) -> &str {
        "mock"
    }

    async fn execute(
        &self,
        _prompt: &str,
        sink: &dyn ProgressSink,
    ) -> Result<String, BackendError> {
        sink.emit(ExecEvent::new("DECISION", "mock backend 运行中")).await;
        Ok(self.output.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecSink {
        events: Mutex<Vec<ExecEvent>>,
    }
    #[async_trait]
    impl ProgressSink for RecSink {
        async fn emit(&self, ev: ExecEvent) {
            self.events.lock().unwrap().push(ev);
        }
    }

    #[tokio::test]
    async fn mock_emits_and_returns() {
        let b = MockBackend { output: "DOC".into() };
        let sink = RecSink::default();
        let out = b.execute("p", &sink).await.expect("run");
        assert_eq!(out, "DOC");
        assert_eq!(sink.events.lock().unwrap().len(), 1);
        assert_eq!(b.cli_name(), "mock");
    }
}

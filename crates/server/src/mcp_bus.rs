//! MCP 服务端推送总线:把任务/交付/验证生命周期事件广播给订阅 `GET /mcp` SSE 的客户端。
//! 进程内 broadcast;无订阅者时发布即丢弃(send 的 Err 忽略)。

use tokio::sync::broadcast;

#[derive(Clone, serde::Serialize)]
pub struct McpEvent {
    pub kind: &'static str,
    pub status: String,
    #[serde(rename = "attemptId", skip_serializing_if = "String::is_empty")]
    pub attempt_id: String,
    #[serde(rename = "taskId", skip_serializing_if = "String::is_empty")]
    pub task_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Clone)]
pub struct McpBus {
    tx: broadcast::Sender<McpEvent>,
}

impl Default for McpBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl McpBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }
    pub fn publish(&self, ev: McpEvent) {
        let _ = self.tx.send(ev);
    }
    pub fn subscribe(&self) -> broadcast::Receiver<McpEvent> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscriber_receives_published_event() {
        let bus = McpBus::default();
        let mut rx = bus.subscribe();
        bus.publish(McpEvent {
            kind: "delivery",
            status: "delivered".into(),
            attempt_id: "a1".into(),
            task_id: "t1".into(),
            message: "done".into(),
        });
        let ev = rx.recv().await.expect("recv");
        assert_eq!(ev.kind, "delivery");
        assert_eq!(ev.attempt_id, "a1");
        let json = serde_json::to_string(&ev).expect("json");
        assert!(json.contains("\"attemptId\":\"a1\""), "{json}");
        assert!(json.contains("\"kind\":\"delivery\""), "{json}");
    }

    #[tokio::test]
    async fn publish_without_subscribers_is_a_noop() {
        let bus = McpBus::default();
        bus.publish(McpEvent {
            kind: "task",
            status: "verified".into(),
            attempt_id: String::new(),
            task_id: "t".into(),
            message: String::new(),
        });
        // 无订阅者:不 panic;empty 字段被 skip。
        let mut rx = bus.subscribe();
        bus.publish(McpEvent {
            kind: "task",
            status: "verified".into(),
            attempt_id: String::new(),
            task_id: "t".into(),
            message: String::new(),
        });
        let json = serde_json::to_string(&rx.recv().await.expect("recv")).expect("json");
        assert!(!json.contains("attemptId"), "empty attemptId skipped: {json}");
    }
}

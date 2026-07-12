//! MCP server-push bus: broadcasts task/delivery/verification lifecycle events to
//! clients subscribed to `GET /mcp` SSE. In-process broadcast; with no subscribers
//! a publish is simply dropped (send's Err is ignored).

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
        // No subscribers: no panic; empty fields are skipped.
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

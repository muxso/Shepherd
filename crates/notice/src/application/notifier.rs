use std::sync::Arc;

use crate::domain::{parse_mentions, NewNotice};
use crate::ports::{NoticeStore, NoticeUserDirectory};

/// Everything about a notification except who receives it.
#[derive(Debug, Clone, Default)]
pub struct NoticeEvent {
    pub project_id: String,
    pub category: String,
    pub event_type: String,
    pub title: String,
    pub content: String,
    pub resource_type: String,
    pub resource_id: String,
    pub operator: String,
}

/// Producer facade injected into other contexts' routers. Notifications are
/// best-effort: failures are logged, never propagated to the calling request.
#[derive(Clone)]
pub struct Notifier {
    store: Arc<dyn NoticeStore>,
    directory: Arc<dyn NoticeUserDirectory>,
}

impl Notifier {
    pub fn new(store: Arc<dyn NoticeStore>, directory: Arc<dyn NoticeUserDirectory>) -> Self {
        Self { store, directory }
    }

    /// Sends the event to explicit receivers; returns rows written.
    pub async fn notify(&self, receivers: Vec<String>, event: NoticeEvent) -> usize {
        self.deliver(receivers, event, false).await
    }

    /// Sends the event to every member of the event's project; when the project
    /// has no member list, falls back to `fallback_receiver` (e.g. the creator).
    pub async fn notify_project_members(
        &self,
        fallback_receiver: &str,
        event: NoticeEvent,
    ) -> usize {
        let mut receivers = match self.directory.project_member_ids(&event.project_id).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(project = %event.project_id, "notice: member lookup failed: {e}");
                Vec::new()
            }
        };
        if receivers.is_empty() {
            receivers = vec![fallback_receiver.to_string()];
        }
        self.deliver(receivers, event, false).await
    }

    /// Parses `@name` mentions out of `text`, resolves them against the user
    /// directory and notifies the matches with `at_mention = true`.
    pub async fn notify_mentions(&self, text: &str, event: NoticeEvent) -> usize {
        let names = parse_mentions(text);
        if names.is_empty() {
            return 0;
        }
        let receivers = match self.directory.resolve_user_ids(&names).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!("notice: mention resolution failed: {e}");
                return 0;
            }
        };
        self.deliver(receivers, event, true).await
    }

    async fn deliver(&self, receivers: Vec<String>, event: NoticeEvent, at_mention: bool) -> usize {
        let notice = NewNotice {
            project_id: event.project_id.trim().to_string(),
            receivers,
            category: event.category,
            event_type: event.event_type,
            title: event.title,
            content: event.content,
            resource_type: event.resource_type,
            resource_id: event.resource_id,
            operator: event.operator,
            at_mention,
        };
        let notice = match notice.validated() {
            Ok(n) => n,
            // No receivers / blank fields: nothing to send, not an error worth logging.
            Err(_) => return 0,
        };
        match self.store.insert(&notice).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(event = %notice.event_type, "notice: store insert failed: {e}");
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::in_memory::{InMemoryNoticeStore, InMemoryUserDirectory};
    use crate::ports::{ListQuery, Tab};

    fn event(category: &str, event_type: &str) -> NoticeEvent {
        NoticeEvent {
            project_id: "p1".into(),
            category: category.into(),
            event_type: event_type.into(),
            title: "登录页崩溃".into(),
            content: String::new(),
            resource_type: "BUG".into(),
            resource_id: "b1".into(),
            operator: "admin".into(),
        }
    }

    fn services() -> (Arc<InMemoryNoticeStore>, Notifier) {
        let store = Arc::new(InMemoryNoticeStore::new());
        let dir = Arc::new(
            InMemoryUserDirectory::new()
                .with_user("u-admin", &["admin"])
                .with_user("u-bob", &["bob"])
                .with_member("p1", "u-admin")
                .with_member("p1", "u-bob"),
        );
        (store.clone(), Notifier::new(store, dir))
    }

    #[tokio::test]
    async fn notify_writes_one_row_per_receiver() {
        let (store, notifier) = services();
        let n = notifier.notify(vec!["u1".into(), "u2".into()], event("BUG", "BUG_ASSIGNED")).await;
        assert_eq!(n, 2);
        let page = store
            .list(&ListQuery {
                receiver_id: "u1".into(),
                page: 1,
                page_size: 10,
                ..Default::default()
            })
            .await
            .expect("list");
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].event_type, "BUG_ASSIGNED");
    }

    #[tokio::test]
    async fn notify_without_receivers_is_a_noop() {
        let (_store, notifier) = services();
        assert_eq!(notifier.notify(vec![" ".into()], event("BUG", "X")).await, 0);
    }

    #[tokio::test]
    async fn project_members_receive_broadcast_with_creator_fallback() {
        let (store, notifier) = services();
        assert_eq!(
            notifier.notify_project_members("creator", event("CASE", "REVIEW_CREATED")).await,
            2
        );
        // Unknown project: falls back to the creator.
        let mut e = event("CASE", "REVIEW_CREATED");
        e.project_id = "ghost".into();
        assert_eq!(notifier.notify_project_members("creator", e).await, 1);
        let page = store
            .list(&ListQuery {
                receiver_id: "creator".into(),
                page: 1,
                page_size: 10,
                ..Default::default()
            })
            .await
            .expect("list");
        assert_eq!(page.total, 1);
    }

    #[tokio::test]
    async fn mentions_resolve_to_user_ids_and_set_at_mention() {
        let (store, notifier) = services();
        let n = notifier.notify_mentions("@admin @ghost 看一下", event("BUG", "MENTIONED")).await;
        assert_eq!(n, 1);
        let page = store
            .list(&ListQuery {
                receiver_id: "u-admin".into(),
                tab: Tab::At,
                page: 1,
                page_size: 10,
                ..Default::default()
            })
            .await
            .expect("list");
        assert_eq!(page.total, 1);
        assert!(page.items[0].at_mention);
    }
}

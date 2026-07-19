use std::sync::Arc;

use crate::ports::{ListQuery, NoticePage, NoticeStore, RepoError, UnreadCount};

const MAX_PAGE_SIZE: u32 = 200;

/// Read side of the message center; always scoped to one receiver.
#[derive(Clone)]
pub struct NoticeQueryService {
    store: Arc<dyn NoticeStore>,
}

impl NoticeQueryService {
    pub fn new(store: Arc<dyn NoticeStore>) -> Self {
        Self { store }
    }

    pub async fn list(&self, mut query: ListQuery) -> Result<NoticePage, RepoError> {
        query.page = query.page.max(1);
        query.page_size = query.page_size.clamp(1, MAX_PAGE_SIZE);
        self.store.list(&query).await
    }

    pub async fn unread_count(
        &self,
        receiver_id: &str,
        project_id: Option<&str>,
    ) -> Result<UnreadCount, RepoError> {
        self.store.unread_count(receiver_id, project_id).await
    }

    pub async fn mark_read(&self, id: &str, receiver_id: &str) -> Result<bool, RepoError> {
        self.store.mark_read(id, receiver_id).await
    }

    pub async fn mark_all_read(
        &self,
        receiver_id: &str,
        project_id: Option<&str>,
    ) -> Result<u64, RepoError> {
        self.store.mark_all_read(receiver_id, project_id).await
    }
}

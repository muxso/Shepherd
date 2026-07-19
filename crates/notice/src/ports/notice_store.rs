use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{NewNotice, Notice};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RepoError {
    #[error("storage backend error: {0}")]
    Backend(String),
}

/// Message-center tab filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    All,
    /// `@me` mentions only.
    At,
    Unread,
    Read,
}

impl Tab {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "all" | "" => Some(Tab::All),
            "at" => Some(Tab::At),
            "unread" => Some(Tab::Unread),
            "read" => Some(Tab::Read),
            _ => None,
        }
    }
}

/// List filter: receiver-scoped; project/category optional.
/// Notices without a project (`project_id = ''`) always match.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub receiver_id: String,
    pub project_id: Option<String>,
    pub category: Option<String>,
    pub tab: Tab,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticePage {
    pub items: Vec<Notice>,
    pub total: u64,
}

/// Unread totals per category (only categories with unread messages appear).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnreadCount {
    pub total: u64,
    pub by_category: Vec<(String, u64)>,
}

#[async_trait]
pub trait NoticeStore: Send + Sync {
    /// Fans the notice out to every receiver; returns the number of rows written.
    async fn insert(&self, notice: &NewNotice) -> Result<usize, RepoError>;

    async fn list(&self, query: &ListQuery) -> Result<NoticePage, RepoError>;

    async fn unread_count(
        &self,
        receiver_id: &str,
        project_id: Option<&str>,
    ) -> Result<UnreadCount, RepoError>;

    /// Marks one message read; false when the id doesn't belong to the receiver.
    async fn mark_read(&self, id: &str, receiver_id: &str) -> Result<bool, RepoError>;

    /// Marks all the receiver's messages read (optionally project-scoped); returns count.
    async fn mark_all_read(
        &self,
        receiver_id: &str,
        project_id: Option<&str>,
    ) -> Result<u64, RepoError>;
}

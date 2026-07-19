use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{NewNotice, Notice};
use crate::ports::{
    ListQuery, NoticePage, NoticeStore, NoticeUserDirectory, RepoError, Tab, UnreadCount,
};

#[derive(Default)]
struct State {
    notices: Vec<Notice>,
    seq: i64,
}

#[derive(Clone, Default)]
pub struct InMemoryNoticeStore {
    state: Arc<Mutex<State>>,
}

impl InMemoryNoticeStore {
    pub fn new() -> Self {
        Self::default()
    }
}

fn matches(n: &Notice, q: &ListQuery) -> bool {
    if n.receiver_id != q.receiver_id {
        return false;
    }
    // Unscoped notices (empty project) show up in every project.
    if let Some(p) = &q.project_id {
        if !n.project_id.is_empty() && &n.project_id != p {
            return false;
        }
    }
    if let Some(c) = &q.category {
        if &n.category != c {
            return false;
        }
    }
    match q.tab {
        Tab::All => true,
        Tab::At => n.at_mention,
        Tab::Unread => !n.read,
        Tab::Read => n.read,
    }
}

#[async_trait]
impl NoticeStore for InMemoryNoticeStore {
    async fn insert(&self, notice: &NewNotice) -> Result<usize, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for r in &notice.receivers {
            st.seq += 1;
            let n = Notice {
                id: format!("notice-{}", st.seq),
                project_id: notice.project_id.clone(),
                receiver_id: r.clone(),
                category: notice.category.clone(),
                event_type: notice.event_type.clone(),
                title: notice.title.clone(),
                content: notice.content.clone(),
                resource_type: notice.resource_type.clone(),
                resource_id: notice.resource_id.clone(),
                operator: notice.operator.clone(),
                at_mention: notice.at_mention,
                read: false,
                created_at: st.seq,
            };
            st.notices.push(n);
        }
        Ok(notice.receivers.len())
    }

    async fn list(&self, query: &ListQuery) -> Result<NoticePage, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut hits: Vec<Notice> =
            st.notices.iter().filter(|n| matches(n, query)).cloned().collect();
        hits.sort_by_key(|n| std::cmp::Reverse(n.created_at));
        let total = hits.len() as u64;
        let start = ((query.page.max(1) - 1) * query.page_size) as usize;
        let items = hits.into_iter().skip(start).take(query.page_size as usize).collect();
        Ok(NoticePage { items, total })
    }

    async fn unread_count(
        &self,
        receiver_id: &str,
        project_id: Option<&str>,
    ) -> Result<UnreadCount, RepoError> {
        let q = ListQuery {
            receiver_id: receiver_id.to_string(),
            project_id: project_id.map(str::to_string),
            tab: Tab::Unread,
            ..Default::default()
        };
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut by: BTreeMap<String, u64> = BTreeMap::new();
        let mut total = 0;
        for n in st.notices.iter().filter(|n| matches(n, &q)) {
            total += 1;
            *by.entry(n.category.clone()).or_default() += 1;
        }
        Ok(UnreadCount { total, by_category: by.into_iter().collect() })
    }

    async fn mark_read(&self, id: &str, receiver_id: &str) -> Result<bool, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match st.notices.iter_mut().find(|n| n.id == id && n.receiver_id == receiver_id) {
            Some(n) => {
                n.read = true;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn mark_all_read(
        &self,
        receiver_id: &str,
        project_id: Option<&str>,
    ) -> Result<u64, RepoError> {
        let q = ListQuery {
            receiver_id: receiver_id.to_string(),
            project_id: project_id.map(str::to_string),
            tab: Tab::Unread,
            ..Default::default()
        };
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut n = 0;
        for notice in st.notices.iter_mut() {
            if matches(notice, &q) {
                notice.read = true;
                n += 1;
            }
        }
        Ok(n)
    }
}

/// Test directory: fixed name→user-id aliases and project membership.
#[derive(Clone, Default)]
pub struct InMemoryUserDirectory {
    aliases: HashMap<String, String>,
    members: HashMap<String, Vec<String>>,
}

impl InMemoryUserDirectory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a user id plus the names that resolve to it.
    pub fn with_user(mut self, user_id: &str, names: &[&str]) -> Self {
        self.aliases.insert(user_id.to_string(), user_id.to_string());
        for n in names {
            self.aliases.insert(n.to_string(), user_id.to_string());
        }
        self
    }

    pub fn with_member(mut self, project_id: &str, user_id: &str) -> Self {
        self.members.entry(project_id.to_string()).or_default().push(user_id.to_string());
        self
    }
}

#[async_trait]
impl NoticeUserDirectory for InMemoryUserDirectory {
    async fn resolve_user_ids(&self, names: &[String]) -> Result<Vec<String>, RepoError> {
        let mut seen = HashSet::new();
        Ok(names
            .iter()
            .filter_map(|n| self.aliases.get(n).cloned())
            .filter(|id| seen.insert(id.clone()))
            .collect())
    }

    async fn project_member_ids(&self, project_id: &str) -> Result<Vec<String>, RepoError> {
        Ok(self.members.get(project_id).cloned().unwrap_or_default())
    }
}

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

// ---------- Routing rules (robots + per-event rules) ----------

use crate::domain::{Robot, RobotDraft, Rule, RuleDraft};
use crate::ports::{NoticeRuleStore, RobotDelivery, RobotSender};

#[derive(Default)]
struct RuleState {
    robots: Vec<Robot>,
    rules: Vec<Rule>,
    seq: i64,
}

#[derive(Clone, Default)]
pub struct InMemoryRuleStore {
    state: Arc<Mutex<RuleState>>,
}

impl InMemoryRuleStore {
    pub fn new() -> Self {
        Self::default()
    }
}

fn robot_from_draft(id: String, created_at: i64, d: &RobotDraft) -> Robot {
    Robot {
        id,
        project_id: d.project_id.clone(),
        name: d.name.clone(),
        platform: d.platform,
        webhook_url: d.webhook_url.clone(),
        secret: d.secret.clone(),
        enabled: d.enabled,
        created_at,
    }
}

fn rule_from_draft(id: String, created_at: i64, d: &RuleDraft) -> Rule {
    Rule {
        id,
        project_id: d.project_id.clone(),
        event_type: d.event_type.clone(),
        channels: d.channels.clone(),
        robot_ids: d.robot_ids.clone(),
        template: d.template.clone(),
        enabled: d.enabled,
        created_at,
    }
}

#[async_trait]
impl NoticeRuleStore for InMemoryRuleStore {
    async fn insert_robot(&self, draft: &RobotDraft) -> Result<Robot, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        st.seq += 1;
        let robot = robot_from_draft(format!("robot-{}", st.seq), st.seq, draft);
        st.robots.push(robot.clone());
        Ok(robot)
    }

    async fn update_robot(&self, id: &str, draft: &RobotDraft) -> Result<Option<Robot>, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match st.robots.iter_mut().find(|r| r.id == id && r.project_id == draft.project_id) {
            Some(r) => {
                *r = robot_from_draft(r.id.clone(), r.created_at, draft);
                Ok(Some(r.clone()))
            }
            None => Ok(None),
        }
    }

    async fn delete_robot(&self, id: &str, project_id: &str) -> Result<bool, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = st.robots.len();
        st.robots.retain(|r| !(r.id == id && r.project_id == project_id));
        Ok(st.robots.len() < before)
    }

    async fn list_robots(&self, project_id: &str) -> Result<Vec<Robot>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.robots.iter().filter(|r| r.project_id == project_id).cloned().collect())
    }

    async fn get_robot(&self, id: &str, project_id: &str) -> Result<Option<Robot>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.robots.iter().find(|r| r.id == id && r.project_id == project_id).cloned())
    }

    async fn robots_by_ids(&self, ids: &[String]) -> Result<Vec<Robot>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.robots.iter().filter(|r| ids.contains(&r.id)).cloned().collect())
    }

    async fn insert_rule(&self, draft: &RuleDraft) -> Result<Rule, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        st.seq += 1;
        let rule = rule_from_draft(format!("rule-{}", st.seq), st.seq, draft);
        st.rules.push(rule.clone());
        Ok(rule)
    }

    async fn update_rule(&self, id: &str, draft: &RuleDraft) -> Result<Option<Rule>, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match st.rules.iter_mut().find(|r| r.id == id && r.project_id == draft.project_id) {
            Some(r) => {
                *r = rule_from_draft(r.id.clone(), r.created_at, draft);
                Ok(Some(r.clone()))
            }
            None => Ok(None),
        }
    }

    async fn delete_rule(&self, id: &str, project_id: &str) -> Result<bool, RepoError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = st.rules.len();
        st.rules.retain(|r| !(r.id == id && r.project_id == project_id));
        Ok(st.rules.len() < before)
    }

    async fn list_rules(&self, project_id: &str) -> Result<Vec<Rule>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st.rules.iter().filter(|r| r.project_id == project_id).cloned().collect())
    }

    async fn rules_for_event(
        &self,
        project_id: &str,
        event_type: &str,
    ) -> Result<Vec<Rule>, RepoError> {
        let st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(st
            .rules
            .iter()
            .filter(|r| {
                r.project_id == project_id
                    && r.enabled
                    && (r.event_type == event_type || r.event_type == "*")
            })
            .cloned()
            .collect())
    }
}

/// Test sender: records every (robot id, text) pair; optionally fails the
/// first N sends to exercise the retry path.
#[derive(Default)]
pub struct RecordingRobotSender {
    sent: Mutex<Vec<(String, String)>>,
    fail_first: Mutex<u32>,
}

impl RecordingRobotSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fail_first(self, n: u32) -> Self {
        *self.fail_first.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = n;
        self
    }

    pub fn sent(&self) -> Vec<(String, String)> {
        self.sent.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

#[async_trait]
impl RobotSender for RecordingRobotSender {
    async fn send(&self, robot: &Robot, text: &str) -> Result<RobotDelivery, String> {
        {
            let mut fail =
                self.fail_first.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if *fail > 0 {
                *fail -= 1;
                return Err("simulated failure".into());
            }
        }
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((robot.id.clone(), text.to_string()));
        Ok(RobotDelivery { status: 200, body: "{\"errcode\":0}".into() })
    }
}

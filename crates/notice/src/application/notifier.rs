use std::sync::Arc;

use crate::application::rule_admin::now_text;
use crate::domain::{
    parse_mentions, render_template, Channel, NewNotice, ReportCard, ReportContext, Robot, Rule,
};
use crate::ports::{NoticeRuleStore, NoticeStore, NoticeUserDirectory, RobotSender};

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

impl NoticeEvent {
    fn into_notice(self, receivers: Vec<String>, at_mention: bool) -> NewNotice {
        NewNotice {
            project_id: self.project_id.trim().to_owned(),
            receivers,
            category: self.category,
            event_type: self.event_type,
            title: self.title,
            content: self.content,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            operator: self.operator,
            at_mention,
        }
    }
}

/// Producer facade injected into other contexts' routers. Notifications are
/// best-effort: failures are logged, never propagated to the calling request.
#[derive(Clone)]
pub struct Notifier {
    store: Arc<dyn NoticeStore>,
    directory: Arc<dyn NoticeUserDirectory>,
    rules: Option<RuleEngine>,
}

impl Notifier {
    pub fn new(store: Arc<dyn NoticeStore>, directory: Arc<dyn NoticeUserDirectory>) -> Self {
        Self { store, directory, rules: None }
    }

    /// Enables server-side routing rules: per-event channel selection (in-app
    /// inbox / robot webhooks). Without this every event goes to the inbox.
    pub fn with_rules(
        mut self,
        rules: Arc<dyn NoticeRuleStore>,
        sender: Arc<dyn RobotSender>,
    ) -> Self {
        self.rules = Some(RuleEngine { store: rules, sender });
        self
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

    pub async fn notify_project_if_configured(
        &self,
        event: NoticeEvent,
    ) -> Result<bool, crate::ports::RepoError> {
        let Some(engine) = &self.rules else { return Ok(false) };
        let rules = engine.store.rules_for_event(&event.project_id, &event.event_type).await?;
        if rules.is_empty() {
            let explicitly_configured = engine
                .store
                .list_rules(&event.project_id)
                .await?
                .iter()
                .any(|rule| rule.event_type == event.event_type);
            return Ok(explicitly_configured);
        }
        let in_app = rules.iter().any(|rule| rule.channels.contains(&Channel::InApp));
        let receivers = if in_app {
            self.directory.project_member_ids(&event.project_id).await?
        } else {
            Vec::new()
        };
        let notice = event.into_notice(receivers, false);
        if let Ok(inbox_notice) = notice.clone().validated() {
            self.store.insert(&inbox_notice).await?;
        }
        engine.dispatch_robots(&rules, &notice, None).await;
        Ok(true)
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
        let notice = event.into_notice(receivers, at_mention);
        let mut notice = match notice.validated() {
            Ok(n) => n,
            // No receivers / blank fields: nothing to send, not an error worth logging.
            Err(_) => return 0,
        };
        // Routing rules: unscoped notices (no project) skip them and stay in-app.
        let mut in_app = true;
        if let Some(engine) = self.rules.as_ref().filter(|_| !notice.project_id.is_empty()) {
            let context = if is_report_event(&notice.event_type) {
                engine.store.report_context(&notice).await
            } else {
                None
            };
            if let Some(context) = &context {
                notice.operator = context.trigger.clone();
            }
            match engine.store.rules_for_event(&notice.project_id, &notice.event_type).await {
                // No rules for this event: default behavior (in-app only).
                Ok(rules) if rules.is_empty() => {}
                Ok(rules) => {
                    in_app = rules.iter().any(|r| r.channels.contains(&Channel::InApp));
                    engine.dispatch_robots(&rules, &notice, context.as_ref()).await;
                }
                Err(e) => {
                    tracing::warn!(
                        event = %notice.event_type,
                        "notice: rule lookup failed, defaulting to in-app: {e}"
                    );
                }
            }
        }
        if !in_app {
            return 0;
        }
        match self.store.insert(&notice).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(event = %notice.event_type, "notice: store insert failed: {e}");
                0
            }
        }
    }
}

/// Robot half of the routing rules; deliveries are fire-and-forget so the
/// producer's request never waits on a webhook.
#[derive(Clone)]
struct RuleEngine {
    store: Arc<dyn NoticeRuleStore>,
    sender: Arc<dyn RobotSender>,
}

impl RuleEngine {
    /// Sends the rendered template of every ROBOT-channel rule to its enabled
    /// robots. Robot fan-out happens once per event, not per receiver.
    async fn dispatch_robots(
        &self,
        rules: &[Rule],
        notice: &NewNotice,
        context: Option<&ReportContext>,
    ) {
        let time = now_text();
        let base_url = if is_report_event(&notice.event_type) {
            self.store.report_base_url().await
        } else {
            None
        };
        let report = ReportCard::from_notice(notice, base_url.as_deref(), &time);
        for rule in
            rules.iter().filter(|r| r.channels.contains(&Channel::Robot) && !r.robot_ids.is_empty())
        {
            let text = match context {
                Some(context) if !rule.template.trim().is_empty() => {
                    context.interpolate(&rule.template, &notice.title, &notice.content, &time)
                }
                Some(context) => context.interpolate(
                    "${project} · ${scenario}\n${content}\n时间 ${time}",
                    &notice.title,
                    &notice.content,
                    &time,
                ),
                None => {
                    let template = if rule.template.trim().is_empty() {
                        "${title}\n${content}\n操作人 ${operator}\n时间 ${time}"
                    } else {
                        &rule.template
                    };
                    render_template(template, &notice.title, &notice.operator, &time)
                        .replace("${content}", &notice.content)
                }
            };
            let mut report = report.clone();
            if let (Some(context), Some(report)) = (context, &mut report) {
                report.project.clone_from(&context.project);
                if !context.scenario.is_empty() {
                    report.title.clone_from(&context.scenario);
                }
                if ReportContext::uses_context(&rule.template) {
                    report.template_summary = context.interpolate(
                        &rule.template,
                        &notice.title,
                        &report.details.join("\n"),
                        &time,
                    );
                }
            }
            let robots = match self.store.robots_by_ids(&rule.robot_ids).await {
                Ok(robots) => robots,
                Err(e) => {
                    tracing::warn!(rule = %rule.id, "notice: robot lookup failed: {e}");
                    continue;
                }
            };
            for robot in robots.into_iter().filter(|r| r.enabled) {
                let sender = self.sender.clone();
                let text = text.clone();
                let report = report.clone();
                tokio::spawn(async move { send_with_retry(sender, robot, text, report).await });
            }
        }
    }
}

fn is_report_event(event: &str) -> bool {
    matches!(
        event,
        "SCENARIO_RUN_SUCCEEDED"
            | "SCENARIO_RUN_FAILED"
            | "PLAN_RUN_FINISHED"
            | "PLAN_SCHEDULE_FAILED"
    )
}

/// One retry on failure; the outcome only shows up in the logs.
async fn send_with_retry(
    sender: Arc<dyn RobotSender>,
    robot: Robot,
    text: String,
    report: Option<ReportCard>,
) {
    for attempt in 1..=2u32 {
        let delivery = match &report {
            Some(report) => sender.send_report(&robot, &text, report).await,
            None => sender.send(&robot, &text).await,
        };
        match delivery {
            Ok(d) if (200..300).contains(&d.status) => {
                tracing::info!(robot = %robot.name, status = d.status, "notice: robot webhook delivered");
                return;
            }
            Ok(d) => {
                tracing::warn!(robot = %robot.name, status = d.status, attempt, body = %d.body, "notice: robot webhook non-2xx");
            }
            Err(e) => {
                tracing::warn!(robot = %robot.name, attempt, "notice: robot webhook failed: {e}");
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
            title: "login page crash".into(),
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
        let n =
            notifier.notify_mentions("@admin @ghost take a look", event("BUG", "MENTIONED")).await;
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

#[cfg(test)]
mod rule_tests {
    use super::*;
    use crate::adapters::in_memory::{
        InMemoryNoticeStore, InMemoryRuleStore, InMemoryUserDirectory, RecordingRobotSender,
    };
    use crate::domain::{RobotDraft, RuleDraft};
    use crate::ports::{ListQuery, NoticeRuleStore};

    fn event(event_type: &str) -> NoticeEvent {
        NoticeEvent {
            project_id: "p1".into(),
            category: "BUG".into(),
            event_type: event_type.into(),
            title: "login page crash".into(),
            content: String::new(),
            resource_type: "BUG".into(),
            resource_id: "b1".into(),
            operator: "admin".into(),
        }
    }

    struct Fixture {
        store: Arc<InMemoryNoticeStore>,
        rules: Arc<InMemoryRuleStore>,
        sender: Arc<RecordingRobotSender>,
        notifier: Notifier,
    }

    fn fixture(sender: RecordingRobotSender) -> Fixture {
        let store = Arc::new(InMemoryNoticeStore::new());
        let rules = Arc::new(InMemoryRuleStore::new());
        let sender = Arc::new(sender);
        let notifier = Notifier::new(
            store.clone(),
            Arc::new(InMemoryUserDirectory::new().with_user("u1", &["u1"])),
        )
        .with_rules(rules.clone(), sender.clone());
        Fixture { store, rules, sender, notifier }
    }

    async fn add_robot(rules: &InMemoryRuleStore, enabled: bool) -> String {
        let robot = rules
            .insert_robot(&RobotDraft {
                project_id: "p1".into(),
                name: "bot".into(),
                platform: crate::domain::Platform::Feishu,
                webhook_url: "https://hook".into(),
                secret: String::new(),
                enabled,
            })
            .await
            .expect("robot");
        robot.id
    }

    async fn add_rule(
        rules: &InMemoryRuleStore,
        event_type: &str,
        channels: Vec<Channel>,
        robot_ids: Vec<String>,
        template: &str,
    ) {
        rules
            .insert_rule(&RuleDraft {
                project_id: "p1".into(),
                event_type: event_type.into(),
                channels,
                robot_ids,
                template: template.into(),
                enabled: true,
            })
            .await
            .expect("rule");
    }

    async fn wait_sent(sender: &RecordingRobotSender, n: usize) -> Vec<(String, String)> {
        for _ in 0..100 {
            let sent = sender.sent();
            if sent.len() >= n {
                return sent;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        sender.sent()
    }

    async fn inbox_total(store: &InMemoryNoticeStore) -> u64 {
        store
            .list(&ListQuery {
                receiver_id: "u1".into(),
                page: 1,
                page_size: 10,
                ..Default::default()
            })
            .await
            .expect("list")
            .total
    }

    #[tokio::test]
    async fn no_rules_keeps_default_in_app_delivery() {
        let f = fixture(RecordingRobotSender::new());
        assert_eq!(f.notifier.notify(vec!["u1".into()], event("BUG_ASSIGNED")).await, 1);
        assert_eq!(inbox_total(&f.store).await, 1);
        assert!(f.sender.sent().is_empty());
    }

    #[tokio::test]
    async fn robot_only_rule_skips_inbox_and_hits_robot() {
        let f = fixture(RecordingRobotSender::new());
        let robot_id = add_robot(&f.rules, true).await;
        add_rule(
            &f.rules,
            "BUG_ASSIGNED",
            vec![Channel::Robot],
            vec![robot_id.clone()],
            "${title} / ${operator}",
        )
        .await;
        assert_eq!(f.notifier.notify(vec!["u1".into()], event("BUG_ASSIGNED")).await, 0);
        assert_eq!(inbox_total(&f.store).await, 0);
        let sent = wait_sent(&f.sender, 1).await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, robot_id);
        assert_eq!(sent[0].1, "login page crash / admin");
    }

    #[tokio::test]
    async fn both_channels_deliver_everywhere_and_wildcard_matches() {
        let f = fixture(RecordingRobotSender::new());
        let robot_id = add_robot(&f.rules, true).await;
        add_rule(&f.rules, "*", vec![Channel::InApp, Channel::Robot], vec![robot_id], "").await;
        assert_eq!(f.notifier.notify(vec!["u1".into()], event("BUG_STATUS_CHANGED")).await, 1);
        assert_eq!(inbox_total(&f.store).await, 1);
        let sent = wait_sent(&f.sender, 1).await;
        // Default template: title + operator + time.
        assert!(sent[0].1.contains("login page crash") && sent[0].1.contains("admin"));
    }

    #[tokio::test]
    async fn disabled_robots_are_skipped_and_unscoped_notices_ignore_rules() {
        let f = fixture(RecordingRobotSender::new());
        let robot_id = add_robot(&f.rules, false).await;
        add_rule(&f.rules, "BUG_ASSIGNED", vec![Channel::Robot], vec![robot_id], "").await;
        assert_eq!(f.notifier.notify(vec!["u1".into()], event("BUG_ASSIGNED")).await, 0);
        // Unscoped event: rules skipped, in-app as usual.
        let mut ev = event("BUG_ASSIGNED");
        ev.project_id = String::new();
        assert_eq!(f.notifier.notify(vec!["u1".into()], ev).await, 1);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(f.sender.sent().is_empty());
    }

    #[tokio::test]
    async fn robot_send_retries_once_on_failure() {
        let f = fixture(RecordingRobotSender::new().fail_first(1));
        let robot_id = add_robot(&f.rules, true).await;
        add_rule(&f.rules, "BUG_ASSIGNED", vec![Channel::Robot], vec![robot_id], "").await;
        f.notifier.notify(vec!["u1".into()], event("BUG_ASSIGNED")).await;
        // First attempt fails, the retry lands.
        assert_eq!(wait_sent(&f.sender, 1).await.len(), 1);
    }
}

#[cfg(test)]
mod project_rule_tests {
    use super::*;
    use crate::adapters::{
        InMemoryNoticeStore, InMemoryRuleStore, InMemoryUserDirectory, RecordingRobotSender,
    };
    use crate::domain::{Platform, RobotDraft, RuleDraft};
    use crate::ports::ListQuery;

    #[tokio::test]
    async fn project_rule_routes_code_quality_and_can_mute_legacy_delivery() {
        let inbox = Arc::new(InMemoryNoticeStore::new());
        let rules = Arc::new(InMemoryRuleStore::new());
        let sender = Arc::new(RecordingRobotSender::new());
        let notifier = Notifier::new(
            inbox.clone(),
            Arc::new(InMemoryUserDirectory::new().with_member("project", "member")),
        )
        .with_rules(rules.clone(), sender.clone());
        let event = NoticeEvent {
            project_id: "project".into(),
            category: "SONAR".into(),
            title: "SonarQube".into(),
            event_type: "SONAR_ANALYSIS_COMPLETED".into(),
            content: "Gate failed".into(),
            ..Default::default()
        };
        assert!(!notifier.notify_project_if_configured(event.clone()).await.unwrap());

        let robot = rules
            .insert_robot(&RobotDraft {
                project_id: event.project_id.clone(),
                name: "Project robot".into(),
                platform: Platform::Wecom,
                webhook_url: "https://example.invalid/robot".into(),
                secret: String::new(),
                enabled: true,
            })
            .await
            .unwrap();
        let mut draft = RuleDraft {
            project_id: event.project_id.clone(),
            event_type: event.event_type.clone(),
            channels: vec![Channel::InApp, Channel::Robot],
            robot_ids: vec![robot.id.clone()],
            template: "${title}: ${content}".into(),
            enabled: false,
        };
        let rule = rules.insert_rule(&draft).await.unwrap();
        assert!(notifier.notify_project_if_configured(event.clone()).await.unwrap());
        let query = ListQuery {
            receiver_id: "member".into(),
            page: 1,
            page_size: 10,
            ..Default::default()
        };
        assert_eq!(inbox.list(&query).await.unwrap().total, 0);

        draft.enabled = true;
        rules.update_rule(&rule.id, &draft).await.unwrap();
        assert!(notifier.notify_project_if_configured(event).await.unwrap());
        assert_eq!(inbox.list(&query).await.unwrap().total, 1);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while sender.sent().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(sender.sent(), vec![(robot.id, "SonarQube: Gate failed".into())]);
    }
}

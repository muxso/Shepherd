use std::{sync::Arc, time::Duration};

use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{Datelike, FixedOffset, Timelike, Utc};
use notice::{
    adapters::robot_sender::ReqwestRobotSender,
    domain::{Platform, Robot},
    ports::RobotSender,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;
use webauth::{AuthUser, SessionStore};
use yunxiao::{
    domain::{
        bug_statistics::BugStatisticsQuery,
        team_activity::{ActivityPermissions, ActivityQuery, ActivityRole},
        ListQuery, MemberMetricsQuery,
    },
    ports::Store,
};

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    statistics: Arc<dyn Store>,
    sessions: Arc<dyn SessionStore>,
    sender: ReqwestRobotSender,
}

impl FromRef<AppState> for Arc<dyn SessionStore> {
    fn from_ref(state: &AppState) -> Self {
        state.sessions.clone()
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum Event {
    Overview,
    Radar,
    Delivery,
    Bugs,
    Authors,
    Dwell,
    Records,
}

impl Event {
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "概览",
            Self::Radar => "个人雷达指标",
            Self::Delivery => "需求产出",
            Self::Bugs => "缺陷统计",
            Self::Authors => "团队产出",
            Self::Dwell => "状态停留时间",
            Self::Records => "明细记录",
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Rule {
    id: Uuid,
    event: Event,
    time: String,
    weekdays: Vec<u32>,
    group: Option<String>,
    record_kind: Option<String>,
    status: Option<String>,
    minimum_matches: u64,
    enabled: bool,
}

impl Rule {
    fn minute(&self) -> Option<u32> {
        let (hour, minute) = self.time.split_once(':')?;
        if hour.len() != 2 || minute.len() != 2 {
            return None;
        }
        let hour: u32 = hour.parse().ok()?;
        let minute: u32 = minute.parse().ok()?;
        (hour < 24 && minute < 60).then_some(hour * 60 + minute)
    }

    fn valid(&self) -> bool {
        self.minute().is_some()
            && !self.weekdays.is_empty()
            && self.weekdays.len() <= 7
            && self.weekdays.iter().all(|day| *day < 7)
            && self.minimum_matches <= 1_000_000
            && self.record_kind.as_deref().is_none_or(|kind| match self.event {
                Event::Dwell => matches!(kind, "requirement" | "bug"),
                Event::Records => matches!(kind, "requirement" | "bug" | "commit" | "testcase"),
                _ => false,
            })
            && self
                .status
                .as_ref()
                .is_none_or(|value| value.len() <= 80 && matches!(self.event, Event::Records))
            && self.group.as_ref().is_none_or(|group| {
                group.len() <= 128
                    && !group.trim().is_empty()
                    && !matches!(self.event, Event::Dwell)
            })
    }

    fn due(&self, weekday: u32, minute: u32) -> bool {
        self.enabled
            && self.weekdays.contains(&weekday)
            && self.minute().is_some_and(|start| (start..start + 5).contains(&minute))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    configured: bool,
    revision: i64,
    rules: Vec<Rule>,
    recent: Vec<Run>,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct Run {
    rule_id: Uuid,
    local_date: chrono::NaiveDate,
    outcome: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Update {
    webhook: Option<String>,
    revision: i64,
    rules: Vec<Rule>,
}

fn authorized(user: &AuthUser) -> Result<(), StatusCode> {
    if user.can("SYSTEM_USER", "UPDATE")
        && ["DEVOPS", "PROJECT", "REQUIREMENT", "BUG", "FUNCTIONAL_CASE"]
            .iter()
            .all(|resource| user.can(resource, "READ"))
    {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn storage(error: sqlx::Error) -> StatusCode {
    tracing::warn!(%error, "could not access DevOps notification settings");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn valid_webhook(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("qyapi.weixin.qq.com")
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/cgi-bin/webhook/send"
            && url.fragment().is_none()
            && url.query_pairs().any(|(key, value)| key == "key" && !value.is_empty())
    })
}

async fn load(state: &AppState, source: &str) -> Result<Settings, StatusCode> {
    let row = sqlx::query_as::<_, (bool, i64, sqlx::types::Json<Vec<Rule>>)>(
        "SELECT webhook_url <> '', revision, rules FROM ms_devops_notification WHERE source_id = $1",
    ).bind(source).fetch_optional(&state.pool).await.map_err(storage)?;
    let (configured, revision, sqlx::types::Json(rules)) =
        row.unwrap_or((false, 0, sqlx::types::Json(Vec::new())));
    let recent = sqlx::query_as(
        "SELECT DISTINCT ON (rule_id) rule_id, local_date, outcome
         FROM ms_devops_notification_run WHERE source_id = $1
         ORDER BY rule_id, local_date DESC",
    )
    .bind(source)
    .fetch_all(&state.pool)
    .await
    .map_err(storage)?;
    Ok(Settings { configured, revision, rules, recent })
}

async fn get_settings(
    user: AuthUser,
    State(state): State<AppState>,
    Path(source): Path<String>,
) -> Result<Json<Settings>, StatusCode> {
    authorized(&user)?;
    Ok(Json(load(&state, &source).await?))
}

async fn save_settings(
    user: AuthUser,
    State(state): State<AppState>,
    Path(source): Path<String>,
    Json(body): Json<Update>,
) -> Result<Json<Settings>, StatusCode> {
    authorized(&user)?;
    let webhook = body.webhook.as_deref().map(str::trim);
    let mut ids = std::collections::HashSet::new();
    if source.len() > 128
        || webhook
            .is_some_and(|value| value.len() > 512 || (!value.is_empty() && !valid_webhook(value)))
        || body.rules.len() > 20
        || body.rules.iter().any(|rule| !rule.valid() || !ids.insert(rule.id))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let sources = state.statistics.sources().await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !sources["items"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["id"] == source))
    {
        return Err(StatusCode::NOT_FOUND);
    }
    let result = if body.revision == 0 {
        sqlx::query(
            "INSERT INTO ms_devops_notification (source_id, webhook_url, rules)
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(&source)
        .bind(webhook.unwrap_or_default())
        .bind(sqlx::types::Json(&body.rules))
        .execute(&state.pool)
        .await
    } else {
        sqlx::query(
            "UPDATE ms_devops_notification SET webhook_url = COALESCE($2, webhook_url),
               rules = $3, revision = revision + 1 WHERE source_id = $1 AND revision = $4",
        )
        .bind(&source)
        .bind(webhook)
        .bind(sqlx::types::Json(&body.rules))
        .bind(body.revision)
        .execute(&state.pool)
        .await
    }
    .map_err(storage)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::CONFLICT);
    }
    Ok(Json(load(&state, &source).await?))
}

pub fn router(pool: PgPool, sessions: Arc<dyn SessionStore>, statistics: Arc<dyn Store>) -> Router {
    let state = AppState { pool, statistics, sessions, sender: ReqwestRobotSender::new() };
    tokio::spawn(run(state.clone()));
    Router::new()
        .route(
            "/api/devops/sources/{id}/notification-settings",
            get(get_settings).put(save_settings),
        )
        .with_state(state)
}

fn n(data: &Value, key: &str) -> u64 {
    data[key].as_u64().unwrap_or_default()
}

fn items<'a>(data: &'a Value, key: &str, group: Option<&str>) -> Vec<&'a Value> {
    data[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| group.is_none_or(|group| item["group"].as_str() == Some(group)))
        .collect()
}

fn name(data: &Value, key: &str) -> String {
    data[key].as_str().unwrap_or("—").chars().filter(|c| !c.is_control()).take(70).collect()
}

async fn summarize(
    store: &dyn Store,
    source: &str,
    rule: &Rule,
) -> Result<(u64, String), yunxiao::domain::Error> {
    let group = rule.group.as_deref();
    match rule.event {
        Event::Overview if group.is_none() => {
            let data = store.dashboard(source, 7).await?;
            let metrics = &data["productivity"]["metrics"];
            let current = |key: &str| n(&metrics[key], "current");
            let (requirements, bugs, commits) =
                (current("requirements"), current("bugs"), current("commits"));
            Ok((
                requirements + bugs + commits,
                format!("近 7 天新增需求 {requirements} · 缺陷 {bugs} · 提交 {commits}"),
            ))
        }
        Event::Radar | Event::Delivery => {
            let data = store
                .member_metrics(
                    source,
                    MemberMetricsQuery { month: None, requirement_target: None },
                )
                .await?;
            let members = items(&data["metrics"], "members", group);
            let mut lines = Vec::new();
            let mut count = 0;
            for member in members {
                let delivered = n(member, "deliveredRequirements");
                if matches!(rule.event, Event::Delivery) && delivered == 0 {
                    continue;
                }
                count += 1;
                if lines.len() < 12 {
                    let person = name(member, "name");
                    let rate = |value: &Value| {
                        value.as_f64().map_or("—".to_owned(), |v| format!("{v:.1}%"))
                    };
                    lines.push(if matches!(rule.event, Event::Radar) {
                        format!(
                            "{person} · 达成 {} · 无关联缺陷 {} · 按期 {} · Sonar 覆盖 {}",
                            rate(&member["radar"][0]),
                            rate(&member["radar"][1]),
                            rate(&member["onTimeRate"]),
                            rate(
                                member
                                    .pointer("/sonar/recent/0/analysisQuality/coverage")
                                    .filter(|value| !value.is_null())
                                    .unwrap_or(&member["sonar"]["recent"][0]["newCoverage"])
                            )
                        )
                    } else {
                        format!("{person} · 交付 {delivered} 项")
                    });
                }
            }
            Ok((count, format!("本月 · 匹配 {count} 位成员\n{}", lines.join("\n"))))
        }
        Event::Bugs => {
            let data = store
                .bug_statistics(
                    source,
                    BugStatisticsQuery {
                        from: None,
                        through: None,
                        days: Some(7),
                        person: None,
                        group: rule.group.clone(),
                        person_role: None,
                    },
                )
                .await?;
            let total = n(&data, "total");
            Ok((
                total,
                format!(
                    "近 7 天新增缺陷 {total} · 关联需求 {} · 未指派 {}",
                    n(&data, "linked"),
                    n(&data, "unassigned")
                ),
            ))
        }
        Event::Authors | Event::Overview => {
            let permissions =
                ActivityPermissions { requirements: true, bugs: true, testcases: true };
            let snapshot = |role| {
                store.team_activity(source, ActivityQuery { days: Some(7), role }, permissions)
            };
            let (code, product, test) = tokio::try_join!(
                snapshot(ActivityRole::Code),
                snapshot(ActivityRole::Product),
                snapshot(ActivityRole::Test)
            )?;
            let mut seen = std::collections::HashSet::new();
            let events: Vec<_> = [&code, &product, &test]
                .into_iter()
                .flat_map(|data| items(data, "events", group))
                .filter(|event| event["id"].as_str().is_some_and(|id| seen.insert(id)))
                .collect();
            let commits = events.iter().filter(|event| event["kind"] == "commit").count();
            let delivered =
                events.iter().filter(|event| event["kind"] == "requirementDelivered").count();
            let requirements =
                events.iter().filter(|event| event["kind"] == "requirementCreated").count();
            let tests = events.iter().filter(|event| event["kind"] == "testcaseCreated").count();
            let total = events.len();
            Ok((
                total as u64,
                format!("近 7 天 · 活动 {total} · 提交 {commits} · 新增需求 {requirements} · 交付活动 {delivered} · 新增用例 {tests}"),
            ))
        }
        Event::Dwell | Event::Records => {
            let dwell = matches!(rule.event, Event::Dwell);
            let entity = rule.record_kind.as_deref().unwrap_or("requirement");
            let kind = if dwell { format!("status-dwell-{entity}") } else { entity.to_owned() };
            let data = store
                .records(
                    source,
                    ListQuery {
                        kind: Some(kind),
                        status: rule.status.clone(),
                        days: Some(7),
                        group: rule.group.clone(),
                        page_size: Some(8),
                        ..Default::default()
                    },
                )
                .await?;
            let rows = items(&data, "items", None);
            if dwell {
                let total = rows.iter().map(|row| n(row, "count")).sum::<u64>();
                let lines = rows
                    .iter()
                    .take(8)
                    .map(|row| {
                        let hours = row["averageSeconds"].as_f64().unwrap_or_default() / 3600.0;
                        let phase =
                            if row["phase"] == "completed" { "已结束" } else { "进行中" };
                        format!(
                            "{} · {phase} · 平均 {hours:.1} 小时 · {} 条",
                            name(row, "status"),
                            n(row, "count")
                        )
                    })
                    .collect::<Vec<_>>();
                return Ok((total, format!("状态停留 · {total} 条观测\n{}", lines.join("\n"))));
            }
            let total = n(&data, "total");
            let lines = rows
                .iter()
                .map(|row| {
                    let payload = row.get("payload").unwrap_or(row);
                    format!("{} · {}", name(payload, "title"), name(payload, "status"))
                })
                .collect::<Vec<_>>();
            Ok((total, format!("明细 · {total} 条\n{}", lines.join("\n"))))
        }
    }
}

async fn tick(state: &AppState) -> Result<(), sqlx::Error> {
    let offset = FixedOffset::east_opt(8 * 3600).expect("Shanghai offset");
    let now = Utc::now().with_timezone(&offset);
    let date = now.date_naive();
    let minute = now.hour() * 60 + now.minute();
    let weekday = now.weekday().num_days_from_monday();
    let rows = sqlx::query_as::<_, (String, String, sqlx::types::Json<Vec<Rule>>)>(
        "SELECT source_id, webhook_url, rules FROM ms_devops_notification WHERE webhook_url <> ''",
    )
    .fetch_all(&state.pool)
    .await?;
    for (source, webhook, sqlx::types::Json(rules)) in rows {
        if !valid_webhook(&webhook) {
            continue;
        }
        for rule in rules.iter().filter(|rule| rule.due(weekday, minute)) {
            let claim = sqlx::query(
                "INSERT INTO ms_devops_notification_run (source_id, rule_id, local_date, outcome)
                VALUES ($1, $2, $3, 'running') ON CONFLICT DO NOTHING",
            )
            .bind(&source)
            .bind(rule.id)
            .bind(date)
            .execute(&state.pool)
            .await?;
            if claim.rows_affected() == 0 {
                continue;
            }
            let outcome = match summarize(state.statistics.as_ref(), &source, rule).await {
                Ok((count, text)) if count >= rule.minimum_matches => {
                    let robot = Robot {
                        id: source.clone(),
                        project_id: String::new(),
                        name: "云效研发统计".into(),
                        platform: Platform::Wecom,
                        webhook_url: webhook.clone(),
                        secret: String::new(),
                        enabled: true,
                        created_at: 0,
                    };
                    let group = rule.group.as_deref().unwrap_or("全部分组");
                    let text =
                        format!("云效研发统计 · {}\n{date} · {group}\n{text}", rule.event.title());
                    let text: String = text.chars().take(650).collect();
                    if state
                        .sender
                        .send(&robot, &text)
                        .await
                        .is_ok_and(|result| (200..300).contains(&result.status))
                    {
                        "sent"
                    } else {
                        "delivery_failed"
                    }
                }
                Ok(_) => "condition_not_met",
                Err(_) => "query_failed",
            };
            sqlx::query(
                "UPDATE ms_devops_notification_run SET outcome = $4
                 WHERE source_id = $1 AND rule_id = $2 AND local_date = $3",
            )
            .bind(&source)
            .bind(rule.id)
            .bind(date)
            .bind(outcome)
            .execute(&state.pool)
            .await?;
            if matches!(outcome, "query_failed" | "delivery_failed") {
                tracing::warn!(%source, rule = %rule.id, outcome, "DevOps notification failed");
            }
        }
    }
    sqlx::query("DELETE FROM ms_devops_notification_run WHERE local_date < $1 - 90")
        .bind(date)
        .execute(&state.pool)
        .await?;
    sqlx::query(
        "UPDATE ms_devops_notification_run SET outcome = 'interrupted'
         WHERE outcome = 'running' AND started_at < now() - interval '1 hour'",
    )
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn run(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if let Err(error) = tick(&state).await {
            tracing::warn!(%error, "DevOps notification scheduler failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_due_enabled_rules_match_a_shanghai_slot() {
        let mut rule = Rule {
            id: Uuid::nil(),
            event: Event::Overview,
            time: "09:00".into(),
            weekdays: vec![0, 4],
            group: Some("研发组".into()),
            record_kind: None,
            status: None,
            minimum_matches: 4,
            enabled: true,
        };
        assert!(rule.valid());
        assert!(rule.due(0, 540));
        assert!(rule.due(4, 544));
        assert!(!rule.due(6, 540));
        assert!(!rule.due(0, 545));
        rule.enabled = false;
        assert!(!rule.due(0, 540));
        rule.time = "24:00".into();
        assert!(!rule.valid());
    }

    #[test]
    fn webhook_must_point_to_wecom() {
        assert!(valid_webhook("https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=test"));
        assert!(!valid_webhook(
            "https://qyapi.weixin.qq.com.invalid/cgi-bin/webhook/send?key=test"
        ));
        assert!(!valid_webhook("https://qyapi.weixin.qq.com/cgi-bin/webhook/send"));
    }
}

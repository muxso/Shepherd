//! Snapshot-based monthly participation metrics; never an employee responsibility score.
use super::bug_quality::{build_bug_quality, BugMetric, BugQuality};
use super::{Error, Record};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "http", derive(utoipa::IntoParams))]
pub struct MemberMetricsQuery {
    pub month: Option<String>,
    pub requirement_target: Option<f64>,
}
impl MemberMetricsQuery {
    pub fn validate(&self) -> Result<(), Error> {
        if let Some(month) = &self.month {
            month_start(month)?;
        }
        let target = self.requirement_target.unwrap_or(10.0);
        if !target.is_finite() || target <= 0.0 || target > 10_000.0 {
            return Err(Error::Invalid("requirementTarget must be > 0 and <= 10000".into()));
        }
        Ok(())
    }
}
fn month_start(month: &str) -> Result<NaiveDate, Error> {
    if month.len() != 7
        || month.as_bytes().get(4) != Some(&b'-')
        || month.bytes().enumerate().any(|(index, byte)| index != 4 && !byte.is_ascii_digit())
    {
        return Err(Error::Invalid("month must be YYYY-MM".into()));
    }
    NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d")
        .ok()
        .filter(|date| (1..=9998).contains(&date.year()))
        .ok_or_else(|| Error::Invalid("month must be a valid YYYY-MM".into()))
}
pub(super) fn text<'a>(data: &'a Value, key: &str) -> &'a str {
    data.get(key).and_then(Value::as_str).unwrap_or("").trim()
}
pub(super) fn human(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && !["自动化规则", "系统", "机器人", "未指派", "待归属"].contains(&name))
        .then(|| name.to_owned())
}
fn members(data: &Value) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(owner) = human(text(data, "owner")) {
        names.insert(owner);
    }
    // A role label alone is not proof of identity: text fields may contain customer
    // names or entire requirement descriptions. Only typed user references qualify.
    if let Some(fields) = data.get("customFieldValues").and_then(Value::as_array) {
        for field in fields {
            if ["开发者", "测试人员", "协同人", "协作人", "需求提出人"]
                .contains(&text(field, "fieldName"))
                && matches!(text(field, "fieldFormat"), "user" | "multiUser")
            {
                for value in field.get("values").and_then(Value::as_array).into_iter().flatten() {
                    let identifier = text(value, "identifier");
                    if identifier.is_empty() || identifier.starts_with("AK-") {
                        continue;
                    }
                    if let Some(name) = human(text(value, "displayValue")) {
                        names.insert(name);
                    }
                }
            }
        }
    }
    names
}
pub(super) fn timestamp(data: &Value, key: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(text(data, key)).ok()
}
pub(super) fn rate(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then(|| (numerator as f64 * 1000.0 / denominator as f64).round() / 10.0)
}
const CANCELLED: &[&str] = &["已取消", "已拒绝", "已关闭", "已完成", "已验收", "完成"];
const INVALID_BUG: &[&str] = &[
    "已取消",
    "已拒绝",
    "已拒绝（不修复）",
    "不予修复",
    "非缺陷",
    "重复缺陷",
    "设计如此",
    "无法重现",
];
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementMetric {
    pub external_id: String,
    pub serial: String,
    pub title: String,
    pub status: String,
    pub due: String,
    pub delivered: bool,
    pub new_in_month: bool,
    pub delivery_class: String,
    pub bug_ids: BTreeSet<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberMetric {
    pub name: String,
    pub group: String,
    pub new_requirements: usize,
    pub delivered_requirements: usize,
    pub linked_bugs: usize,
    pub bug_affected_requirements: usize,
    pub bug_rate: Option<f64>,
    pub reopen_count: usize,
    pub reopened_bugs: usize,
    pub reopen_evaluated_bugs: usize,
    pub reopen_rate: Option<f64>,
    pub bugs: Vec<BugMetric>,
    pub processed_bugs: usize,
    pub quality_bugs: usize,
    pub reopen_unknown_bugs: usize,
    pub due_requirements: usize,
    pub on_time: usize,
    pub delivered_late: usize,
    pub overdue_at_end: usize,
    pub unknown_delivery: usize,
    pub delay_rate: Option<f64>,
    pub radar: [Option<f64>; 3],
    pub requirements: Vec<RequirementMetric>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberMetrics {
    pub month: String,
    pub start: String,
    pub end_exclusive: String,
    pub timezone: &'static str,
    pub requirement_target: f64,
    pub members: Vec<MemberMetric>,
    pub new_valid_bugs: usize,
    pub linked_valid_bugs: usize,
    pub requirements_missing_due: usize,
    pub requirements_missing_time: usize,
    pub snapshot_based: bool,
    pub bug_quality: BugQuality,
}

/// Caller provides one atomic, active-source snapshot and its database clock.
pub fn build_member_metrics(
    records: &[Record],
    query: &MemberMetricsQuery,
    now: DateTime<FixedOffset>,
) -> Result<MemberMetrics, Error> {
    query.validate()?;
    let tz = FixedOffset::east_opt(8 * 3600).ok_or(Error::Storage)?;
    let now = now.with_timezone(&tz);
    let month = query.month.clone().unwrap_or_else(|| {
        let first = now.date_naive().with_day(1).unwrap_or(now.date_naive());
        (first - Duration::days(1)).format("%Y-%m").to_string()
    });
    let a = month_start(&month)?;
    let b = if a.month() == 12 {
        NaiveDate::from_ymd_opt(a.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(a.year(), a.month() + 1, 1)
    }
    .ok_or(Error::Storage)?;
    let at_midnight = |date: NaiveDate| {
        date.and_hms_opt(0, 0, 0)
            .and_then(|value| value.and_local_timezone(tz).single())
            .ok_or(Error::Storage)
    };
    let start = at_midnight(a)?;
    if start > now {
        return Err(Error::Invalid("month has not started".into()));
    }
    let end = at_midnight(b)?.min(now);
    let inside =
        |value: Option<DateTime<FixedOffset>>| value.is_some_and(|t| start <= t && t < end);
    let target = query.requirement_target.unwrap_or(10.0);
    let mut group_candidates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for record in records {
        let data = &record.data;
        let key = if record.kind == "commit" { "author" } else { "owner" };
        if let Some(name) = human(text(data, key)) {
            let group = text(data, "group");
            if !group.is_empty() && group != "待归属" {
                group_candidates.entry(name).or_default().insert(group.to_owned());
            }
        }
    }
    let mut people: BTreeMap<String, BTreeMap<String, RequirementMetric>> = BTreeMap::new();
    let mut delivered: BTreeMap<String, (BTreeSet<String>, RequirementMetric)> = BTreeMap::new();
    let mut references = BTreeMap::new();
    let mut missing_due = 0;
    let mut missing_time = 0;
    for record in records.iter().filter(|r| r.kind == "requirement") {
        let data = &record.data;
        let created = timestamp(data, "createdAt");
        if created.is_some_and(|at| at >= end) {
            continue;
        }
        let names = members(data);
        let names = if names.is_empty() { BTreeSet::from(["待归属".to_owned()]) } else { names };
        // Keep known zero-activity members visible without assuming a full organization roster.
        for name in &names {
            people.entry(name.clone()).or_default();
        }
        let changed = timestamp(data, "statusChangedAt");
        let published = text(data, "status") == "已发布";
        let is_delivered = published && inside(changed);
        let is_new = inside(created);
        let due = NaiveDate::parse_from_str(text(data, "due"), "%Y-%m-%d").ok();
        if due.is_none() {
            missing_due += 1;
        }
        if created.is_none() || (published && changed.is_none()) {
            missing_time += 1;
        }
        let mut class = "notDue";
        if !CANCELLED.contains(&text(data, "status")) {
            if let Some(due) = due.filter(|due| a <= *due && *due < b) {
                let deadline = at_midnight(due + Duration::days(1))?;
                if deadline <= end {
                    class = if published {
                        match changed {
                            Some(t) if t < deadline => "onTime",
                            Some(t) if t < end => "deliveredLate",
                            Some(_) => "overdueAtEnd",
                            None => "unknown",
                        }
                    } else if text(data, "status").is_empty() || changed.is_none_or(|t| t >= end) {
                        "unknown"
                    } else {
                        "overdueAtEnd"
                    };
                }
            }
        }
        if !is_new && !is_delivered && class == "notDue" {
            continue;
        }
        let row = RequirementMetric {
            external_id: record.external_id.clone(),
            serial: text(data, "serial").to_owned(),
            title: text(data, "title").to_owned(),
            status: text(data, "status").to_owned(),
            due: text(data, "due").to_owned(),
            delivered: is_delivered,
            new_in_month: is_new,
            delivery_class: class.to_owned(),
            bug_ids: BTreeSet::new(),
        };
        for name in &names {
            people.entry(name.clone()).or_default().insert(record.external_id.clone(), row.clone());
        }
        if is_delivered {
            // Exact external IDs take precedence over serial aliases (same source only).
            references.insert(record.external_id.clone(), record.external_id.clone());
            delivered.insert(record.external_id.clone(), (names, row));
        }
    }
    for (id, (_, row)) in &delivered {
        if !row.serial.is_empty() {
            references.entry(row.serial.clone()).or_insert_with(|| id.clone());
        }
    }
    let mut valid_bugs = BTreeSet::new();
    let mut linked_bugs = BTreeSet::new();
    for bug in records.iter().filter(|r| r.kind == "bug") {
        if !inside(timestamp(&bug.data, "createdAt"))
            || INVALID_BUG.contains(&text(&bug.data, "status"))
        {
            continue;
        }
        valid_bugs.insert(bug.external_id.clone());
        let mut refs: BTreeSet<String> =
            [text(&bug.data, "parentId"), text(&bug.data, "parentSerial")]
                .into_iter()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
        refs.extend(
            bug.data
                .get("requirementIds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
        for reference in refs {
            if let Some(id) = references.get(&reference) {
                if let Some((names, _)) = delivered.get(id) {
                    linked_bugs.insert(bug.external_id.clone());
                    for name in names {
                        if let Some(row) = people.get_mut(name).and_then(|rows| rows.get_mut(id)) {
                            row.bug_ids.insert(bug.external_id.clone());
                        }
                    }
                }
            }
        }
    }
    let bug_links_available = records.iter().filter(|r| r.kind == "bug").any(|r| {
        !text(&r.data, "parentId").is_empty()
            || !text(&r.data, "parentSerial").is_empty()
            || r.data.get("requirementIds").and_then(Value::as_array).is_some_and(|a| !a.is_empty())
    });
    let quality =
        build_bug_quality(records, start, end, if now.date_naive() < b { start } else { end });
    for name in quality.by_person.keys() {
        people.entry(name.clone()).or_default();
    }
    let members = people
        .into_iter()
        .map(|(name, rows)| {
            let rows: Vec<_> = rows.into_values().collect();
            let count = |class: &str| rows.iter().filter(|r| r.delivery_class == class).count();
            let delivered_count = rows.iter().filter(|r| r.delivered).count();
            let affected = rows.iter().filter(|r| !r.bug_ids.is_empty()).count();
            let bugs: BTreeSet<_> = rows.iter().flat_map(|r| r.bug_ids.iter()).collect();
            let bug_rows = quality.by_person.get(&name).cloned().unwrap_or_default();
            let evaluated = bug_rows.iter().filter(|bug| bug.history_available).count();
            let unknown_bugs = bug_rows.len() - evaluated;
            let processed = bug_rows.iter().filter(|bug| bug.processed).count();
            let reopened =
                bug_rows.iter().filter(|bug| bug.reopen_count.is_some_and(|n| n > 0)).count();
            let reopen_count = bug_rows.iter().filter_map(|bug| bug.reopen_count).sum();
            let reopen_rate = if unknown_bugs == 0 { rate(reopened, processed) } else { None };
            let due = rows.iter().filter(|r| r.delivery_class != "notDue").count();
            let unknown = count("unknown");
            let bug_rate = if bug_links_available { rate(affected, delivered_count) } else { None };
            let delay_rate = rate(count("deliveredLate") + count("overdueAtEnd"), due - unknown);
            let group = group_candidates
                .get(&name)
                .filter(|groups| groups.len() == 1)
                .and_then(|groups| groups.first())
                .cloned()
                .unwrap_or_else(|| "待归属".into());
            MemberMetric {
                name,
                group,
                new_requirements: rows.iter().filter(|r| r.new_in_month).count(),
                delivered_requirements: delivered_count,
                linked_bugs: bugs.len(),
                bug_affected_requirements: affected,
                bug_rate,
                reopen_count,
                reopened_bugs: reopened,
                reopen_evaluated_bugs: evaluated,
                reopen_rate,
                quality_bugs: bug_rows.len(),
                processed_bugs: processed,
                reopen_unknown_bugs: unknown_bugs,
                bugs: bug_rows,
                due_requirements: due,
                on_time: count("onTime"),
                delivered_late: count("deliveredLate"),
                overdue_at_end: count("overdueAtEnd"),
                unknown_delivery: unknown,
                delay_rate,
                radar: [
                    Some((delivered_count as f64 / target * 100.0).min(100.0)),
                    bug_rate.map(|r| 100.0 - r),
                    delay_rate.map(|r| 100.0 - r),
                ],
                requirements: rows,
            }
        })
        .collect();
    Ok(MemberMetrics {
        month,
        start: start.to_rfc3339(),
        end_exclusive: end.to_rfc3339(),
        timezone: "Asia/Shanghai",
        requirement_target: target,
        members,
        new_valid_bugs: valid_bugs.len(),
        linked_valid_bugs: linked_bugs.len(),
        requirements_missing_due: missing_due,
        requirements_missing_time: missing_time,
        snapshot_based: true,
        bug_quality: quality,
    })
}

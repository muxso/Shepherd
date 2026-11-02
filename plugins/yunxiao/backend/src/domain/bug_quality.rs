//! Defect quality follows the Bug's own dated history and explicit responsibility.
use super::member_metrics::{human, rate, text, timestamp};
use super::Record;
use chrono::{DateTime, FixedOffset};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusTransition {
    pub id: String,
    pub at: String,
    pub from: String,
    pub to: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BugMetric {
    pub external_id: String,
    pub serial: String,
    pub title: String,
    pub status: String,
    pub created_at: String,
    pub bug_type: String,
    pub responsibles: BTreeSet<String>,
    pub developers: BTreeSet<String>,
    pub testers: BTreeSet<String>,
    pub reporter: String,
    pub processed: bool,
    pub history_available: bool,
    pub reopen_count: Option<usize>,
    pub reopen_times: Vec<String>,
    pub history: Vec<StatusTransition>,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BugQuality {
    pub total_source_bugs: usize,
    pub verified_source_bugs: usize,
    pub unknown_source_bugs: usize,
    pub processed_bugs: usize,
    pub reopened_bugs: usize,
    pub reopen_count: usize,
    pub reopen_rate: Option<f64>,
    pub rows: Vec<BugMetric>,
    #[serde(skip)]
    pub by_person: BTreeMap<String, Vec<BugMetric>>,
}
fn people(data: &Value, role: &str) -> BTreeSet<String> {
    data.get("customFieldValues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|f| {
            text(f, "fieldName") == role && matches!(text(f, "fieldFormat"), "user" | "multiUser")
        })
        .flat_map(|f| f.get("values").and_then(Value::as_array).into_iter().flatten())
        .filter(|v| !text(v, "identifier").is_empty() && !text(v, "identifier").starts_with("AK-"))
        .filter_map(|v| human(text(v, "displayValue")))
        .collect()
}
fn field(data: &Value, name: &str) -> String {
    data.get("customFieldValues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|f| text(f, "fieldName") == name)
        .flat_map(|f| f.get("values").and_then(Value::as_array).into_iter().flatten())
        .map(|v| text(v, "displayValue"))
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join("、")
}
struct History {
    transitions: Vec<StatusTransition>,
    reopens: Vec<DateTime<FixedOffset>>,
    resolutions: Vec<DateTime<FixedOffset>>,
}
fn history(data: &Value, coverage: DateTime<FixedOffset>) -> Option<History> {
    let h = data.get("reopenHistory")?;
    if h.get("schemaVersion")?.as_u64()? != 2 || !h.get("complete")?.as_bool()? {
        return None;
    }
    let checked = timestamp(h, "checkedAt")?;
    let created = timestamp(data, "createdAt")?;
    if checked < coverage {
        return None;
    }
    let mut transitions = BTreeMap::new();
    for event in h.get("transitions")?.as_array()? {
        let id = text(event, "id");
        let at = timestamp(event, "at")?;
        if id.is_empty()
            || at < created
            || at > checked
            || text(event, "from").is_empty()
            || text(event, "to").is_empty()
        {
            return None;
        }
        let row = StatusTransition {
            id: id.into(),
            at: at.to_rfc3339(),
            from: text(event, "from").into(),
            to: text(event, "to").into(),
        };
        if transitions.insert(id, row.clone()).is_some_and(|previous| previous != row) {
            return None;
        }
    }
    let project = |key| -> Option<Vec<DateTime<FixedOffset>>> {
        let mut events = BTreeMap::new();
        for event in h.get(key)?.as_array()? {
            let id = text(event, "id");
            let at = timestamp(event, "at")?;
            if DateTime::parse_from_rfc3339(&transitions.get(id)?.at).ok()? != at {
                return None;
            }
            if events.insert(id, at).is_some_and(|previous| previous != at) {
                return None;
            }
        }
        Some(events.into_values().collect())
    };
    let reopens = project("events")?;
    let resolutions = project("resolutions")?;
    Some(History { transitions: transitions.into_values().collect(), reopens, resolutions })
}
pub fn build_bug_quality(
    records: &[Record],
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
    coverage: DateTime<FixedOffset>,
) -> BugQuality {
    let mut result = BugQuality::default();
    let inside = |t: &DateTime<FixedOffset>| start <= *t && *t < end;
    for record in records.iter().filter(|r| r.kind == "bug") {
        let d = &record.data;
        if timestamp(d, "createdAt").is_some_and(|at| at >= end) {
            continue;
        }
        result.total_source_bugs += 1;
        let history = history(d, coverage);
        if history.is_some() {
            result.verified_source_bugs += 1;
        } else {
            result.unknown_source_bugs += 1;
        }
        let mut times: Vec<_> = history
            .as_ref()
            .into_iter()
            .flat_map(|h| &h.reopens)
            .filter(|at| inside(at))
            .map(DateTime::to_rfc3339)
            .collect();
        times.sort_by_key(|at| DateTime::parse_from_rfc3339(at).ok());
        let processed =
            !times.is_empty() || history.as_ref().is_some_and(|h| h.resolutions.iter().any(inside));
        let changed = history.as_ref().is_some_and(|h| {
            h.transitions
                .iter()
                .any(|e| DateTime::parse_from_rfc3339(&e.at).is_ok_and(|at| inside(&at)))
        });
        let new = timestamp(d, "createdAt").is_some_and(|at| inside(&at));
        if !new && !changed && history.is_some() {
            continue;
        }
        let mut responsibles = people(d, "缺陷责任人");
        if responsibles.is_empty() {
            responsibles.insert("待归属".into());
        }
        let mut timeline: Vec<_> = history
            .as_ref()
            .map(|h| h.transitions.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|e| DateTime::parse_from_rfc3339(&e.at).is_ok_and(|at| at < end))
            .collect();
        timeline.sort_by_key(|e| (DateTime::parse_from_rfc3339(&e.at).ok(), e.id.clone()));
        let row = BugMetric {
            external_id: record.external_id.clone(),
            serial: text(d, "serial").into(),
            title: text(d, "title").into(),
            status: text(d, "status").into(),
            created_at: text(d, "createdAt").into(),
            bug_type: field(d, "缺陷类型"),
            responsibles,
            developers: people(d, "开发者"),
            testers: people(d, "测试人员"),
            reporter: text(d, "creator").into(),
            processed,
            history_available: history.is_some(),
            reopen_count: history.as_ref().map(|_| times.len()),
            reopen_times: times,
            history: timeline,
        };
        result.processed_bugs += usize::from(processed);
        result.reopened_bugs += usize::from(row.reopen_count.is_some_and(|n| n > 0));
        result.reopen_count += row.reopen_count.unwrap_or_default();
        for name in &row.responsibles {
            result.by_person.entry(name.clone()).or_default().push(row.clone());
        }
        result.rows.push(row);
    }
    result.reopen_rate = if result.unknown_source_bugs == 0 {
        rate(result.reopened_bugs, result.processed_bugs)
    } else {
        None
    };
    result
}

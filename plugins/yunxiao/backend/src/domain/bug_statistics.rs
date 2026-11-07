use super::{
    author_identities::AuthorIdentities,
    bug_quality::people,
    member_metrics::{human, text, timestamp},
    team_activity::ActivityMembers,
    Error, Record,
};
use chrono::{DateTime, Duration, FixedOffset, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum BugPersonRole {
    #[default]
    Responsible,
    Reporter,
    Assignee,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "http", derive(utoipa::IntoParams))]
#[serde(rename_all = "camelCase")]
pub struct BugStatisticsQuery {
    pub from: Option<String>,
    pub through: Option<String>,
    pub days: Option<u32>,
    pub person: Option<String>,
    pub group: Option<String>,
    pub person_role: Option<BugPersonRole>,
}
impl BugStatisticsQuery {
    pub fn validate(&self) -> Result<(), Error> {
        if self.from.is_some() != self.through.is_some()
            || !(1..=366).contains(&self.days.unwrap_or(7))
        {
            return Err(Error::Invalid("provide both dates and days within 1..366".into()));
        }
        for date in [&self.from, &self.through].into_iter().flatten() {
            if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() || date.len() != 10 {
                return Err(Error::Invalid("dates must be YYYY-MM-DD".into()));
            }
        }
        for name in [&self.person, &self.group].into_iter().flatten() {
            if name.len() > 255 || name.chars().any(char::is_control) {
                return Err(Error::Invalid("invalid person or group".into()));
            }
        }
        Ok(())
    }
}
pub fn build(
    source: &Value,
    query: &BugStatisticsQuery,
    now: DateTime<FixedOffset>,
    records: &[Record],
    identities: &AuthorIdentities,
    roster: &ActivityMembers,
) -> Result<Value, Error> {
    query.validate()?;
    let tz = FixedOffset::east_opt(8 * 3600).ok_or(Error::Storage)?;
    let today = now.with_timezone(&tz).date_naive();
    let through = query
        .through
        .as_deref()
        .map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|_| Error::Invalid("invalid end date".into()))?
        .unwrap_or(today);
    let from = query
        .from
        .as_deref()
        .map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|_| Error::Invalid("invalid start date".into()))?
        .unwrap_or(through - Duration::days(i64::from(query.days.unwrap_or(7)) - 1));
    if through < from || through > today || (through - from).num_days() >= 366 {
        return Err(Error::Invalid("date range must cover 1..366 days through today".into()));
    }
    let canonical = |name: &str| -> String {
        let name = name.trim();
        identities
            .identities
            .iter()
            .find(|i| i.name == name || i.aliases.iter().any(|a| a == name))
            .map(|i| i.name.clone())
            .unwrap_or_else(|| name.into())
    };
    let mut candidates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut references = BTreeSet::new();
    for r in records {
        if r.kind == "requirement" {
            references.insert(r.external_id.clone());
            if !text(&r.data, "serial").is_empty() {
                references.insert(text(&r.data, "serial").into());
            }
        }
        let name =
            if r.kind == "commit" { text(&r.data, "author") } else { text(&r.data, "owner") };
        let group = text(&r.data, "group");
        if !name.is_empty() && !group.is_empty() && group != "待归属" {
            candidates.entry(canonical(name)).or_default().insert(group.into());
        }
    }
    let group_for = |name: &str| -> String {
        identities
            .identities
            .iter()
            .find(|i| i.name == name)
            .and_then(|i| i.group.clone())
            .or_else(|| {
                roster
                    .members
                    .iter()
                    .find(|m| m.enabled && canonical(&m.name) == name)
                    .and_then(|m| m.group.clone())
            })
            .or_else(|| {
                candidates.get(name).filter(|g| g.len() == 1).and_then(|g| g.first().cloned())
            })
            .unwrap_or_else(|| "待归属".into())
    };
    let mut statuses: BTreeMap<String, usize> = BTreeMap::new();
    let mut priorities: BTreeMap<String, usize> = BTreeMap::new();
    let mut daily: BTreeMap<NaiveDate, usize> = BTreeMap::new();
    let mut actors: BTreeMap<String, usize> = BTreeMap::new();
    let mut reporters: BTreeMap<String, usize> = BTreeMap::new();
    let mut options = BTreeSet::new();
    let mut groups = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let (mut total, mut linked, mut unassigned, mut missing_time) = (0, 0, 0, 0);
    for r in records.iter().filter(|r| r.kind == "bug") {
        if !seen.insert(&r.external_id) {
            continue;
        }
        let d = &r.data;
        let Some(at) = timestamp(d, "createdAt") else {
            missing_time += 1;
            continue;
        };
        let day = at.with_timezone(&tz).date_naive();
        if day < from || day > through || at > now {
            continue;
        }
        let mut names: BTreeSet<String> = match query.person_role.unwrap_or_default() {
            BugPersonRole::Responsible => {
                people(d, "缺陷责任人").into_iter().map(|p| canonical(&p)).collect()
            }
            BugPersonRole::Reporter => human(text(d, "creator"))
                .map(|p| BTreeSet::from([canonical(&p)]))
                .unwrap_or_default(),
            BugPersonRole::Assignee => {
                human(text(d, "owner")).map(|p| BTreeSet::from([canonical(&p)])).unwrap_or_default()
            }
        };
        if names.is_empty() {
            names.insert("待归属".into());
        }
        for name in &names {
            options.insert(name.clone());
            groups.insert(group_for(name));
        }
        if !names.iter().any(|name| {
            query.person.as_deref().filter(|s| !s.is_empty()).is_none_or(|p| canonical(p) == *name)
                && query
                    .group
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .is_none_or(|g| group_for(name) == g)
        }) {
            continue;
        }
        total += 1;
        *daily.entry(day).or_default() += 1;
        *priorities
            .entry(if text(d, "priority").is_empty() {
                "未记录".into()
            } else {
                text(d, "priority").into()
            })
            .or_default() += 1;
        let status = if text(d, "status").is_empty() { "未记录" } else { text(d, "status") };
        *statuses.entry(status.into()).or_default() += 1;
        if people(d, "缺陷责任人").is_empty() {
            unassigned += 1;
        }
        let ids = d
            .get("requirementIds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .chain([text(d, "parentId"), text(d, "parentSerial")]);
        if ids.filter(|id| *id != "EMPTY_VALUE" && !id.is_empty()).any(|id| references.contains(id))
        {
            linked += 1;
        }
        for name in names {
            *actors.entry(name).or_default() += 1;
        }
        *reporters
            .entry(
                human(text(d, "creator")).map(|p| canonical(&p)).unwrap_or_else(|| "待归属".into()),
            )
            .or_default() += 1;
    }
    let ranked = |values: BTreeMap<String, usize>, key: &str| {
        let mut rows: Vec<_> = values.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        rows.into_iter()
            .map(|(name, count)| {
                let mut value = json!({"count":count});
                value[key] = json!(name);
                value
            })
            .collect::<Vec<_>>()
    };
    let trend: Vec<_> = (0..=(through - from).num_days())
        .map(|i| {
            let date = from + Duration::days(i);
            json!({"date":date.to_string(),"value":daily.get(&date).copied().unwrap_or(0)})
        })
        .collect();
    Ok(
        json!({"trend":trend,"priorities":ranked(priorities,"name"),"source":source,"from":from.to_string(),"through":through.to_string(),"timezone":"Asia/Shanghai","total":total,"new":total,"unassigned":unassigned,"linked":linked,"unlinked":total-linked,"missingTime":missing_time,"statuses":ranked(statuses,"status"),"reporters":ranked(reporters,"name"),"people":ranked(actors,"name"),"personOptions":options,"groups":groups}),
    )
}

use super::{
    author_identities::{bounded, AuthorIdentities},
    bug_quality::people,
    member_metrics::{human, text, timestamp, INVALID_BUG},
    Error, Record,
};
use chrono::{DateTime, Duration, FixedOffset, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum ActivityRole {
    #[default]
    Code,
    Product,
    Test,
}
impl ActivityRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Product => "product",
            Self::Test => "test",
        }
    }
}
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(feature = "http", derive(utoipa::IntoParams))]
#[serde(rename_all = "camelCase")]
pub struct ActivityQuery {
    pub days: Option<u32>,
    #[serde(default)]
    pub role: ActivityRole,
}
impl ActivityQuery {
    pub fn validate(&self) -> Result<(), Error> {
        if !(1..=365).contains(&self.days.unwrap_or(7)) {
            return Err(Error::Invalid("days must be between 1 and 365".into()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityMember {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    pub roles: Vec<ActivityRole>,
    #[serde(default = "enabled")]
    pub enabled: bool,
}
fn enabled() -> bool {
    true
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "http", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityMembers {
    pub members: Vec<ActivityMember>,
}
impl ActivityMembers {
    pub fn normalize(mut self) -> Result<Self, Error> {
        if self.members.len() > 500 {
            return Err(Error::Invalid("at most 500 members are allowed".into()));
        }
        let mut names = BTreeSet::new();
        for member in &mut self.members {
            member.name = bounded(&member.name)?;
            member.group = member
                .group
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(bounded)
                .transpose()?;
            if !names.insert(member.name.clone()) {
                return Err(Error::Conflict("duplicate member name".into()));
            }
            member.roles.sort();
            member.roles.dedup();
            if member.roles.is_empty() {
                return Err(Error::Invalid("each member needs at least one role".into()));
            }
        }
        self.members.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(self)
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPermissions {
    pub requirements: bool,
    pub bugs: bool,
    pub testcases: bool,
}

pub struct CollectionWindow {
    pub now: DateTime<FixedOffset>,
    pub collected_since: Option<DateTime<FixedOffset>>,
    pub testcases_collected: bool,
}

pub fn build_activity(
    records: &[Record],
    source: &Value,
    query: &ActivityQuery,
    identities: &AuthorIdentities,
    roster: &ActivityMembers,
    permissions: ActivityPermissions,
    window: CollectionWindow,
) -> Result<Value, Error> {
    let CollectionWindow { now, collected_since, testcases_collected } = window;
    query.validate()?;
    let tz = FixedOffset::east_opt(8 * 3600).ok_or(Error::Storage)?;
    let now = now.with_timezone(&tz);
    let today = now.date_naive();
    let from = today - Duration::days(i64::from(query.days.unwrap_or(7)) - 1);
    let start = tz
        .from_local_datetime(&from.and_hms_opt(0, 0, 0).ok_or(Error::Storage)?)
        .single()
        .ok_or(Error::Storage)?;
    let end = tz
        .from_local_datetime(
            &(today + Duration::days(1)).and_hms_opt(0, 0, 0).ok_or(Error::Storage)?,
        )
        .single()
        .ok_or(Error::Storage)?;
    let canonical = |name: &str| {
        identities
            .identities
            .iter()
            .find(|p| p.name == name || p.aliases.iter().any(|a| a == name))
            .map_or_else(|| name.to_owned(), |p| p.name.clone())
    };
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for r in records {
        let actor = if r.kind == "commit" {
            text(&r.data, "author")
        } else if r.kind == "requirement" {
            text(&r.data, "owner")
        } else {
            ""
        };
        let group = text(&r.data, "group");
        if !actor.is_empty() && !group.is_empty() && group != "待归属" {
            groups.entry(canonical(actor)).or_default().insert(group.into());
        }
    }
    let group_for = |name: &str| -> String {
        if let Some(group) = roster
            .members
            .iter()
            .find(|p| p.enabled && canonical(&p.name) == name)
            .and_then(|p| p.group.clone())
        {
            return group;
        }
        if let Some(group) =
            identities.identities.iter().find(|p| p.name == name).and_then(|p| p.group.clone())
        {
            return group;
        }
        groups
            .get(name)
            .filter(|g| g.len() == 1)
            .and_then(|g| g.first())
            .cloned()
            .unwrap_or_else(|| "待归属".into())
    };
    let mut references = BTreeMap::new();
    if permissions.requirements {
        for r in records.iter().filter(|r| r.kind == "requirement") {
            references.insert(r.external_id.clone(),json!({"externalId":r.external_id,"serial":r.data["serial"].as_str(),"title":r.data["title"].as_str(),"url":workitem_url(source,"requirement",&r.external_id,&r.data)}));
        }
        for r in records.iter().filter(|r| r.kind == "requirement") {
            let serial = text(&r.data, "serial");
            if !serial.is_empty() {
                if let Some(value) = references.get(&r.external_id).cloned() {
                    references.entry(serial.into()).or_insert(value);
                }
            }
        }
    }
    let resolve = |ids: Vec<String>| -> Vec<Value> {
        if !permissions.requirements {
            return Vec::new();
        }
        let mut linked = BTreeMap::new();
        for id in ids.into_iter().filter(|s| !s.is_empty()) {
            let value = references
                .get(&id)
                .cloned()
                .unwrap_or_else(|| json!({"externalId":id,"serial":null,"title":null,"url":null}));
            linked.insert(value["externalId"].as_str().unwrap_or(&id).to_owned(), value);
        }
        linked.into_values().collect()
    };
    let mut observed: BTreeMap<String, Option<DateTime<FixedOffset>>> = BTreeMap::new();
    let mut events = Vec::new();
    let mut bugs = Vec::new();
    let mut seen = BTreeSet::new();
    let mut unattributed = 0usize;
    let mut unknown_times = BTreeSet::new();
    for record in records {
        let d = &record.data;
        let mut kinds = Vec::new();
        match (query.role, record.kind.as_str()) {
            (ActivityRole::Code, "commit") => {
                if d["isMerge"].as_bool() == Some(true)
                    || d["parentIds"].as_array().is_some_and(|p| p.len() > 1)
                {
                    continue;
                }
                let repo = d["repoId"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| d["repoId"].as_u64().map(|v| v.to_string()))
                    .unwrap_or_else(|| {
                        let repo = text(d, "repo");
                        if repo.is_empty() {
                            record.external_id.clone()
                        } else {
                            repo.to_owned()
                        }
                    });
                let sha = d["sha"].as_str().unwrap_or(&record.external_id);
                if !seen.insert((repo, sha.to_owned())) {
                    continue;
                }
                kinds.push(("commit", text(d, "author"), "authoredAt"));
            }
            (ActivityRole::Product, "requirement") if permissions.requirements => {
                kinds.push(("requirementCreated", creator(d), "createdAt"));
                if text(d, "status") == "已发布" {
                    kinds.push(("requirementDelivered", text(d, "owner"), "statusChangedAt"));
                }
            }
            (ActivityRole::Test, "testcase") if permissions.testcases => {
                kinds.push(("testcaseCreated", creator(d), "createdAt"))
            }
            (ActivityRole::Test, "bug")
                if permissions.bugs && !INVALID_BUG.contains(&text(d, "status")) =>
            {
                kinds.push(("bugReported", creator(d), "createdAt"))
            }
            _ => {}
        }
        for (kind, raw_actor, field) in kinds {
            let Some(actor) = human(raw_actor).map(|p| canonical(&p)) else {
                if timestamp(d, field).is_some_and(|at| start <= at && at <= now) {
                    unattributed += 1;
                }
                continue;
            };
            let at = timestamp(d, field);
            if at.is_some_and(|at| at > now) {
                continue;
            }
            if at.is_none() {
                unknown_times.insert(actor.clone());
            }
            observed.entry(actor.clone()).and_modify(|last| *last = (*last).max(at)).or_insert(at);
            let Some(at) = at.filter(|at| start <= *at && *at < end) else {
                continue;
            };
            let ids = links(d);
            events.push(json!({"id":format!("{kind}:{}",record.external_id),"externalId":record.external_id,"actor":actor,"authorRaw":d.get("authorRaw").and_then(Value::as_str).unwrap_or(raw_actor),"group":group_for(&actor),"kind":kind,"date":at.with_timezone(&tz).date_naive().to_string(),"at":at.to_rfc3339(),"title":d["title"].as_str(),"serial":d["serial"].as_str(),"status":d["status"].as_str(),"repoId":d["repoId"],"repo":d["repo"],"sha":d["sha"],"additions":d["additions"].as_u64(),"deletions":d["deletions"].as_u64(),"linked":!ids.is_empty(),"requirements":resolve(ids),"url":if record.kind=="commit" {original_url(d)} else {workitem_url(source,&record.kind,&record.external_id,d)},"repositoryUrl":repository_url(d)}));
        }
        if record.kind == "bug"
            && query.role == ActivityRole::Code
            && permissions.bugs
            && !INVALID_BUG.contains(&text(d, "status"))
        {
            if let Some(at) = timestamp(d, "createdAt").filter(|at| start <= *at && *at <= now) {
                let mut responsibles: BTreeSet<_> =
                    people(d, "缺陷责任人").into_iter().map(|p| canonical(&p)).collect();
                if responsibles.is_empty() {
                    responsibles.insert("待归属".into());
                }
                for person in responsibles.iter().filter(|p| p.as_str() != "待归属") {
                    observed.entry(person.clone()).or_default();
                }
                bugs.push(json!({"id":record.external_id,"serial":d["serial"].as_str(),"title":d["title"].as_str(),"status":d["status"].as_str(),"at":at.to_rfc3339(),"date":at.with_timezone(&tz).date_naive().to_string(),"responsibles":responsibles,"requirements":resolve(links(d)),"url":workitem_url(source,"bug",&record.external_id,d)}));
            }
        }
    }
    let success = source
        .get("last_success_at")
        .and_then(Value::as_str)
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok());
    let fresh = success.is_some_and(|at| now - at < Duration::days(1));
    let complete = if query.role == ActivityRole::Code {
        let since = source
            .get("code_since")
            .and_then(Value::as_str)
            .and_then(|v| DateTime::parse_from_rfc3339(v).ok());
        let until = source
            .get("code_until")
            .and_then(Value::as_str)
            .and_then(|v| DateTime::parse_from_rfc3339(v).ok());
        fresh
            && since.is_some_and(|at| at <= start)
            && until.is_some_and(|at| success.is_some_and(|s| at >= s - Duration::minutes(15)))
    } else {
        fresh
            && collected_since.is_some_and(|at| at <= start)
            && (query.role != ActivityRole::Test || testcases_collected)
    };
    let mut names: BTreeSet<_> = observed.keys().cloned().collect();
    let configured: BTreeMap<_, _> = roster
        .members
        .iter()
        .filter(|m| m.enabled && m.roles.contains(&query.role))
        .map(|m| (canonical(&m.name), m))
        .collect();
    names.extend(configured.keys().cloned());
    let excluded: BTreeSet<_> = roster
        .members
        .iter()
        .map(|m| canonical(&m.name))
        .filter(|name| !configured.contains_key(name))
        .collect();
    let mut members = Vec::new();
    for name in names.into_iter().filter(|name| !excluded.contains(name)) {
        let count = events.iter().filter(|e| e["actor"].as_str() == Some(&name)).count();
        let in_roster = configured.contains_key(&name);
        let status = if count > 0 {
            "active"
        } else if in_roster && complete && !unknown_times.contains(&name) {
            "noRecords"
        } else {
            "unverified"
        };
        members.push(json!({"name":name,"group":group_for(&name),"inRoster":in_roster,"eventCount":count,"lastActivityAt":observed.get(&name).copied().flatten().map(|at|at.to_rfc3339()),"status":status}));
    }
    members.sort_by(|a, b| {
        b["eventCount"]
            .as_u64()
            .cmp(&a["eventCount"].as_u64())
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    events.sort_by(|a, b| {
        a["at"]
            .as_str()
            .cmp(&b["at"].as_str())
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    Ok(
        json!({"source":source,"role":query.role,"days":query.days.unwrap_or(7),"from":from.to_string(),"through":today.to_string(),"start":start.to_rfc3339(),"endExclusive":end.to_rfc3339(),"timezone":"Asia/Shanghai","observedAt":success.map(|at|at.to_rfc3339()),"members":members,"events":events,"bugs":bugs,"coverage":{"complete":complete,"rosterConfigured":!configured.is_empty(),"unattributedEvents":unattributed,"testcasesCollected":testcases_collected},"permissions":permissions}),
    )
}
fn creator(data: &Value) -> &str {
    let name = text(data, "creator");
    if !name.is_empty() {
        return name;
    }
    data.pointer("/creatorInfo/name").and_then(Value::as_str).unwrap_or("")
}
fn links(data: &Value) -> Vec<String> {
    let mut ids: Vec<_> = data["requirementIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    for key in ["parentId", "parentSerial"] {
        let id = text(data, key);
        if !id.is_empty() {
            ids.push(id.to_owned());
        }
    }
    ids
}

fn original_url(data: &Value) -> Option<String> {
    ["url", "webUrl", "web_url", "sourceUrl"]
        .iter()
        .filter_map(|key| data.get(key).and_then(Value::as_str))
        .map(str::trim)
        .find(|url| {
            (url.starts_with("https://") || url.starts_with("http://"))
                && !url.chars().any(char::is_control)
        })
        .map(str::to_owned)
}
fn repository_url(data: &Value) -> Option<String> {
    for key in ["repositoryUrl", "repoUrl"] {
        if let Some(url) = data.get(key).and_then(Value::as_str) {
            if let Some(value) = original_url(&json!({"url":url})) {
                return Some(value);
            }
        }
    }
    let url = original_url(data)?;
    let sha = text(data, "sha");
    if sha.is_empty() {
        return None;
    }
    url.strip_suffix(&format!("/commit/{sha}")).filter(|base| !base.is_empty()).map(str::to_owned)
}

fn workitem_url(source: &Value, kind: &str, id: &str, data: &Value) -> Option<String> {
    if let Some(url) = original_url(data) {
        return Some(url);
    }
    let category = match kind {
        "requirement" => "req",
        "bug" => "bug",
        _ => return None,
    };
    let project = source.get("cloud_project_id")?.as_str()?;
    let valid = |s: &str| {
        !s.is_empty()
            && s.len() <= 300
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    };
    if !valid(project) || !valid(id) {
        return None;
    }
    Some(format!("https://devops.aliyun.com/projex/project/{project}/{category}/{id}"))
}

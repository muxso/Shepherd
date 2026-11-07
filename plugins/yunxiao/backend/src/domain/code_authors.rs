use super::{member_metrics::month_start, Error, Record};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "http", derive(utoipa::IntoParams))]
pub struct CodeAuthorsQuery {
    pub month: String,
}
impl CodeAuthorsQuery {
    pub fn bounds(&self) -> Result<(DateTime<FixedOffset>, DateTime<FixedOffset>), Error> {
        let start = month_start(&self.month)?;
        let end = if start.month() == 12 {
            NaiveDate::from_ymd_opt(start.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)
        }
        .ok_or_else(|| Error::Invalid("invalid month".into()))?;
        let tz = FixedOffset::east_opt(8 * 3600).ok_or(Error::Storage)?;
        let at = |date: NaiveDate| -> Result<DateTime<FixedOffset>, Error> {
            tz.from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or(Error::Storage)?)
                .single()
                .ok_or(Error::Storage)
        };
        Ok((at(start)?, at(end)?))
    }
}

/// Thin code facts. Requirement names are resolved once by id/serial in the same source.
pub fn facts(
    commits: Vec<Record>,
    requirements: Vec<Record>,
    can_requirements: bool,
) -> Vec<Value> {
    let mut refs = BTreeMap::new();
    for requirement in requirements {
        let value = json!({"externalId":requirement.external_id,"serial":requirement.data["serial"],"title":requirement.data["title"]});
        refs.insert(requirement.external_id, value.clone());
        if let Some(serial) = requirement.data["serial"].as_str().filter(|s| !s.is_empty()) {
            refs.entry(serial.to_owned()).or_insert(value);
        }
    }
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for commit in commits {
        let data = commit.data;
        if data["isMerge"].as_bool() == Some(true)
            || data["parentIds"].as_array().is_some_and(|p| p.len() > 1)
        {
            continue;
        }
        let repo_id = data["repoId"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| data["repoId"].as_u64().map(|id| id.to_string()))
            .unwrap_or_else(|| data["repo"].as_str().unwrap_or(&commit.external_id).to_owned());
        let sha = data["sha"].as_str().unwrap_or(&commit.external_id);
        if !seen.insert((repo_id.clone(), sha.to_owned())) {
            continue;
        }
        let ids = data["requirementIds"].as_array().cloned().unwrap_or_default();
        let mut linked = BTreeMap::new();
        if can_requirements {
            for id in &ids {
                if let Some(id) = id.as_str().filter(|id| !id.is_empty()) {
                    let value = refs
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| json!({"externalId":id,"serial":null,"title":null}));
                    linked.insert(value["externalId"].as_str().unwrap_or(id).to_owned(), value);
                }
            }
        }
        let languages: BTreeSet<&str> = data["repositoryLanguages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .collect();
        result.push(json!({"id":commit.external_id,"sha":sha,"author":data["author"],"authorRaw":data.get("authorRaw").unwrap_or(&data["author"]),"group":data["group"],"repoId":repo_id,"repo":data["repo"],"authoredAt":data["authoredAt"],
            "additions":data["additions"].as_u64(),"deletions":data["deletions"].as_u64(),"linked":!ids.is_empty(),"requirements":linked.into_values().collect::<Vec<_>>(),
            "languages":languages,"languageSource":data["repositoryLanguageSource"],"languageCommit":data["repositoryLanguageCommit"],"branches":data.get("branches")}));
    }
    result
}

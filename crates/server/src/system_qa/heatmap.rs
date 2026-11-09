use chrono::{DateTime, FixedOffset};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Cell {
    count: u64,
    latest: Option<DateTime<FixedOffset>>,
    additions: Option<u64>,
    deletions: Option<u64>,
    incomplete: bool,
    requirements: BTreeSet<String>,
    bugs: BTreeSet<String>,
    created: u64,
    published: u64,
    cases: u64,
}
impl Cell {
    fn event(&mut self, event: &Value) {
        self.count += 1;
        if let Some(at) = event["at"].as_str().and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        {
            if self.latest.is_none_or(|latest| at > latest) {
                self.latest = Some(at);
            }
        }
        let kind = text(event, "kind");
        if kind == "commit" {
            for (total, value) in [
                (&mut self.additions, &event["additions"]),
                (&mut self.deletions, &event["deletions"]),
            ] {
                if let Some(value) = value.as_u64() {
                    *total = Some(total.unwrap_or_default() + value);
                } else {
                    self.incomplete = true;
                }
            }
        }
        for requirement in event["requirements"].as_array().into_iter().flatten() {
            let id = text(requirement, "externalId");
            if !id.is_empty() {
                self.requirements.insert(id.into());
            }
        }
        if kind.starts_with("requirement") {
            let id = text(event, "externalId");
            if !id.is_empty() {
                self.requirements.insert(id.into());
            }
        }
        match kind {
            "requirementCreated" => self.created += 1,
            "requirementDelivered" => self.published += 1,
            "testcaseCreated" => self.cases += 1,
            "bugReported" => {
                let id = text(event, "externalId");
                if !id.is_empty() {
                    self.bugs.insert(id.into());
                }
            }
            _ => {}
        }
    }
    fn value(&self) -> Value {
        json!({"count":self.count,"lastTime":self.latest.map(|at|at.with_timezone(&FixedOffset::east_opt(28800).expect("Shanghai")).format("%H:%M").to_string()),"additions":self.additions,"deletions":self.deletions,"lineStatsIncomplete":self.incomplete,"requirements":self.requirements.len(),"bugs":self.bugs.len(),"created":self.created,"published":self.published,"testcases":self.cases})
    }
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

pub fn expand(
    rows: &mut [Value],
    dates: &[String],
    events: &[&Value],
    data: &Value,
    role: &str,
    grouped: bool,
) {
    if rows.len().saturating_mul(dates.len()) > 6000 {
        return;
    }
    let mut index: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        index.entry(text(row, "label").into()).or_default().push(i);
    }
    let date_index: BTreeMap<_, _> =
        dates.iter().enumerate().map(|(i, date)| (date.as_str(), i)).collect();
    let mut cells: Vec<Vec<Cell>> =
        (0..rows.len()).map(|_| (0..dates.len()).map(|_| Cell::default()).collect()).collect();
    let mut totals: Vec<Cell> = (0..rows.len()).map(|_| Cell::default()).collect();
    for event in events {
        let Some(&day) = date_index.get(text(event, "date")) else { continue };
        let actor =
            if text(event, "actor").is_empty() { "未关联账号" } else { text(event, "actor") };
        let group =
            if text(event, "group").is_empty() { "待归属" } else { text(event, "group") };
        if let Some(indices) = index.get(if grouped { group } else { actor }) {
            for &i in indices {
                if !grouped && text(&rows[i], "group") != group {
                    continue;
                }
                cells[i][day].event(event);
                totals[i].event(event);
            }
        }
    }
    if role == "code" {
        for bug in data["bugs"].as_array().into_iter().flatten() {
            let Some(&day) = date_index.get(text(bug, "date")) else { continue };
            let id = text(bug, "id");
            if id.is_empty() {
                continue;
            }
            for actor in
                bug["responsibles"].as_array().into_iter().flatten().filter_map(Value::as_str)
            {
                let group = data["members"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|member| text(member, "name") == actor)
                    .map(|member| text(member, "group"))
                    .filter(|group| !group.is_empty())
                    .unwrap_or("待归属");
                if let Some(indices) = index.get(if grouped { group } else { actor }) {
                    for &i in indices {
                        cells[i][day].bugs.insert(id.into());
                        totals[i].bugs.insert(id.into());
                    }
                }
            }
        }
    }
    for (i, row) in rows.iter_mut().enumerate() {
        row["cells"] = json!(cells[i].iter().map(Cell::value).collect::<Vec<_>>());
        row["totals"] = totals[i].value();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cells_keep_last_time_known_lines_and_distinct_requirements_and_bugs() {
        let dates = vec!["2026-11-07".into(), "2026-11-08".into()];
        let mut rows = vec![json!({"label":"Member A","group":"Team","values":[2,1]})];
        let events = [
            json!({"kind":"commit","actor":"Member A","group":"Team","date":"2026-11-07","at":"2026-11-07T09:00:00Z","additions":9,"deletions":6,"requirements":[{"externalId":"r1"}]}),
            json!({"kind":"commit","actor":"Member A","group":"Team","date":"2026-11-07","at":"2026-11-07T18:35:00+08:00","additions":null,"deletions":1,"requirements":[{"externalId":"r1"}]}),
            json!({"kind":"commit","actor":"Member A","group":"Team","date":"2026-11-08","requirements":[{"externalId":"r1"}]}),
        ];
        let data = json!({"bugs":[{"id":"b1","date":"2026-11-07","responsibles":["Member A"]},{"id":"b1","date":"2026-11-07","responsibles":["Member A"]}]});
        expand(&mut rows, &dates, &events.iter().collect::<Vec<_>>(), &data, "code", false);
        assert_eq!(rows[0]["cells"][0]["lastTime"], "18:35");
        assert_eq!(rows[0]["cells"][0]["additions"], 9);
        assert_eq!(rows[0]["cells"][0]["deletions"], 7);
        assert_eq!(rows[0]["cells"][0]["requirements"], 1);
        assert_eq!(rows[0]["totals"]["requirements"], 1);
        assert_eq!(rows[0]["totals"]["bugs"], 1);
        assert_eq!(rows[0]["totals"]["count"], 3);
        assert!(rows[0]["cells"][1]["additions"].is_null());
    }
}

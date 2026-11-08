use chrono::DateTime;
use serde_json::{json, Value};
use shepherd_yunxiao::domain::{
    author_identities::AuthorIdentities,
    bug_statistics::{build, BugPersonRole, BugStatisticsQuery},
    team_activity::ActivityMembers,
    Record,
};
fn record(kind: &str, id: &str, data: Value) -> Record {
    Record { kind: kind.into(), external_id: id.into(), data }
}
fn records() -> Vec<Record> {
    vec![
        record("requirement", "req", json!({"serial":"R-1","owner":"Alice","group":"A"})),
        record("commit", "alice", json!({"author":"Alice","group":"A"})),
        record("commit", "bob", json!({"author":"Bob","group":"B"})),
        record(
            "bug",
            "shared",
            json!({"creator":"Reporter","owner":"Bob","status":"已解决","priority":"高","createdAt":"2026-11-03T16:00:00Z","requirementIds":["R-1"],"customFieldValues":[{"fieldName":"缺陷责任人","fieldFormat":"multiUser","values":[{"identifier":"a","displayValue":"git-a"},{"identifier":"b","displayValue":"Bob"}]}]}),
        ),
        record(
            "bug",
            "unknown",
            json!({"creator":"Reporter","owner":"Bob","status":"开发中","createdAt":"2026-11-04T10:00:00+08:00","parentId":"EMPTY_VALUE"}),
        ),
        record("bug", "earlier", json!({"creator":"Other","createdAt":"2026-11-03T15:59:59Z"})),
        record("bug", "undated", json!({"creator":"Reporter"})),
    ]
}
fn run(query: BugStatisticsQuery) -> Value {
    build(
        &json!({}),
        &query,
        DateTime::parse_from_rfc3339("2026-11-07T20:00:00+08:00").expect("valid fixture"),
        &records(),
        &AuthorIdentities {
            identities: vec![serde_json::from_value(
                json!({"name":"Alice","aliases":["git-a"],"group":"A"}),
            )
            .expect("valid fixture")],
        },
        &ActivityMembers { members: vec![] },
    )
    .expect("valid fixture")
}
fn query() -> BugStatisticsQuery {
    BugStatisticsQuery {
        from: Some("2026-11-04".into()),
        through: Some("2026-11-04".into()),
        ..Default::default()
    }
}
#[test]
fn dates_use_shanghai_days_and_missing_dates_do_not_turn_into_zero() {
    let result = run(query());
    assert_eq!(result["total"], 2);
    assert_eq!(result["linked"], 1);
    assert_eq!(result["unlinked"], 1);
    assert_eq!(result["unassigned"], 1);
    assert_eq!(result["missingTime"], 1);
    assert_eq!(result["trend"], json!([{"date":"2026-11-04","value":2}]));
}
#[test]
fn person_and_group_must_match_the_same_responsible_and_shared_bugs_count_once() {
    let result = run(BugStatisticsQuery {
        person: Some("git-a".into()),
        group: Some("A".into()),
        ..query()
    });
    assert_eq!(result["total"], 1);
    assert_eq!(result["people"].as_array().expect("valid fixture").len(), 2);
    let mismatch = run(BugStatisticsQuery {
        person: Some("Alice".into()),
        group: Some("B".into()),
        ..query()
    });
    assert_eq!(mismatch["total"], 0);
}
#[test]
fn reporter_assignee_and_unknown_responsibility_remain_distinct() {
    for role in [BugPersonRole::Reporter, BugPersonRole::Assignee] {
        let result = run(BugStatisticsQuery {
            person_role: Some(role),
            person: Some(
                if matches!(role, BugPersonRole::Reporter) { "Reporter" } else { "Bob" }.into(),
            ),
            ..query()
        });
        assert_eq!(result["total"], 2);
    }
    let unknown = run(BugStatisticsQuery { person: Some("待归属".into()), ..query() });
    assert_eq!(unknown["total"], 1);
    assert_eq!(unknown["linked"], 0);
}
#[test]
fn rejects_partial_invalid_and_reversed_date_ranges() {
    assert!(BugStatisticsQuery { from: Some("2026-11-04".into()), ..Default::default() }
        .validate()
        .is_err());
    assert!(BugStatisticsQuery { days: Some(0), ..Default::default() }.validate().is_err());
    assert!(build(
        &json!({}),
        &BugStatisticsQuery {
            from: Some("2026-11-05".into()),
            through: Some("2026-11-04".into()),
            ..Default::default()
        },
        DateTime::parse_from_rfc3339("2026-11-07T20:00:00+08:00").expect("valid fixture"),
        &[],
        &AuthorIdentities { identities: vec![] },
        &ActivityMembers { members: vec![] }
    )
    .is_err());
}

use serde_json::{json, Value};
use shepherd_yunxiao::domain::{
    code_authors::{facts, CodeAuthorsQuery},
    Record,
};
fn record(id: &str, data: Value) -> Record {
    Record { external_id: id.into(), kind: "commit".into(), data }
}
#[test]
fn exact_shanghai_month_and_year_rollover() {
    let (start, end) = CodeAuthorsQuery { month: "2026-12".into() }.bounds().expect("bounds");
    assert_eq!(start.to_rfc3339(), "2026-12-01T00:00:00+08:00");
    assert_eq!(end.to_rfc3339(), "2027-01-01T00:00:00+08:00");
    for month in ["2026-13", "2026-1", "0000-01", "invalid"] {
        assert!(CodeAuthorsQuery { month: month.into() }.bounds().is_err());
    }
}
#[test]
fn deduplicates_per_repository_excludes_merges_and_preserves_unknown_lines() {
    let commit = json!({"repoId":1,"repo":"org/api","sha":"abc","author":"甲","authoredAt":"2026-11-01T00:00:00+08:00","additions":0,"requirementIds":["r1","REQ-1","missing"],"repositoryLanguages":["Java","Java",""]});
    let mut other = commit.clone();
    other["repoId"] = json!(2);
    let rows = vec![
        record("a", commit.clone()),
        record("duplicate", commit.clone()),
        record("other", other),
        record("merge", json!({"isMerge":true})),
        record("parents", json!({"parentIds":["p1","p2"]})),
    ];
    let req = record("r1", json!({"serial":"REQ-1","title":"支付申请"}));
    let out = facts(rows.clone(), vec![req.clone()], true);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["additions"], 0);
    assert!(out[0]["deletions"].is_null());
    assert_eq!(out[0]["languages"], json!(["Java"]));
    assert_eq!(out[0]["requirements"].as_array().expect("refs").len(), 2);
    assert!(out[0]["requirements"]
        .as_array()
        .expect("refs")
        .iter()
        .any(|v| v["title"] == "支付申请"));
    let redacted = facts(rows, vec![req], false);
    assert_eq!(redacted[0]["requirements"], json!([]));
    assert_eq!(redacted[0]["linked"], true);
    let fallback = facts(
        vec![
            record("one", json!({"repo":"a","sha":"same"})),
            record("two", json!({"repo":"b","sha":"same"})),
        ],
        vec![],
        true,
    );
    assert_eq!(fallback.len(), 2);
}

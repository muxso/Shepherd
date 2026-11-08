use chrono::{DateTime, FixedOffset};
use serde_json::{json, Value};
use shepherd_yunxiao::domain::{
    author_identities::AuthorIdentities,
    team_activity::{
        build_activity, ActivityMembers, ActivityPermissions, ActivityQuery, ActivityRole,
        CollectionWindow,
    },
    Record,
};
fn at(s: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(s).expect("time")
}
fn record(kind: &str, id: &str, data: Value) -> Record {
    Record { kind: kind.into(), external_id: id.into(), data }
}
fn permissions() -> ActivityPermissions {
    ActivityPermissions { requirements: true, bugs: true, testcases: true }
}
fn source() -> Value {
    json!({"last_success_at":"2026-11-07T12:33:00Z","code_since":"2026-10-01T00:00:00Z","code_until":"2026-11-07T12:31:00Z"})
}
fn run(
    records: &[Record],
    role: ActivityRole,
    roster: Value,
    ids: Value,
    perms: ActivityPermissions,
) -> Value {
    build_activity(
        records,
        &source(),
        &ActivityQuery { days: Some(7), role },
        &serde_json::from_value::<AuthorIdentities>(ids).expect("identities"),
        &serde_json::from_value::<ActivityMembers>(roster).expect("roster"),
        perms,
        CollectionWindow {
            author_groups: vec![],
            now: at("2026-11-07T21:00:00+08:00"),
            collected_since: Some(at("2026-10-01T00:00:00+08:00")),
            testcases_collected: true,
        },
    )
    .expect("activity")
}
#[test]
fn product_and_test_use_real_dated_actors_without_owner_substitution() {
    let records = vec![
        record(
            "requirement",
            "r",
            json!({"title":"real request","creator":"product-creator","owner":"delivery-owner","status":"已发布","createdAt":"2026-11-01T00:00:00+08:00","statusChangedAt":"2026-11-06T00:00:00+08:00","raw":{"description":"private"}}),
        ),
        record(
            "testcase",
            "c",
            json!({"creator":"case-author","owner":"case-owner","createdAt":"2026-11-03T00:00:00+08:00"}),
        ),
        record(
            "bug",
            "b",
            json!({"creator":"bug-reporter","owner":"bug-owner","status":"已解决","createdAt":"2026-11-04T00:00:00+08:00"}),
        ),
        record(
            "bug",
            "invalid",
            json!({"creator":"invalid-reporter","status":"重复缺陷","createdAt":"2026-11-04T00:00:00+08:00"}),
        ),
        record(
            "testcase",
            "unknown-creator",
            json!({"owner":"not-a-creator","createdAt":"2026-11-04T00:00:00+08:00"}),
        ),
    ];
    let roster = json!({"members":[]});
    let ids = json!({"identities":[]});
    let product = run(&records, ActivityRole::Product, roster.clone(), ids.clone(), permissions());
    let actors: Vec<_> = product["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|e| e["actor"].as_str().expect("actor"))
        .collect();
    assert_eq!(actors, vec!["product-creator", "delivery-owner"]);
    let test = run(&records, ActivityRole::Test, roster, ids, permissions());
    let actors: Vec<_> = test["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|e| e["actor"].as_str().expect("actor"))
        .collect();
    assert_eq!(actors, vec!["case-author", "bug-reporter"]);
    assert_eq!(test["coverage"]["unattributedEvents"], 1);
    assert!(!test.to_string().contains("private"));
    assert!(!test.to_string().contains("not-a-creator"));
    let empty = run(
        &[],
        ActivityRole::Product,
        json!({"members":[]}),
        json!({"identities":[]}),
        permissions(),
    );
    assert_eq!(empty["members"], json!([]));
    assert_eq!(empty["events"], json!([]));
}
#[test]
fn aliases_multiple_requirements_and_explicit_bug_responsibility_stay_distinct() {
    let records = vec![
        record(
            "requirement",
            "r1",
            json!({"serial":"R-1","title":"first","url":"https://example.com/r1"}),
        ),
        record("requirement", "r2", json!({"serial":"R-2","title":"second"})),
        record(
            "commit",
            "repo:a",
            json!({"author":"git-a","repoId":"repo","sha":"a","authoredAt":"2026-11-06T16:00:00Z","additions":4,"deletions":2,"requirementIds":["r1","R-1","r2"],"url":"https://codeup.aliyun.com/org/repo/commit/a"}),
        ),
        record(
            "commit",
            "copy",
            json!({"author":"Alice","repoId":"repo","sha":"a","authoredAt":"2026-11-07T00:00:00+08:00"}),
        ),
        record(
            "commit",
            "repo:merge",
            json!({"author":"Alice","repoId":"repo","sha":"merge","isMerge":true,"authoredAt":"2026-11-07T00:00:00+08:00"}),
        ),
        record(
            "bug",
            "bug",
            json!({"creator":"Reporter","owner":"Handler","createdAt":"2026-11-07T00:00:00+08:00","customFieldValues":[{"fieldName":"缺陷责任人","fieldFormat":"multiUser","values":[{"identifier":"u1","displayValue":"git-a"},{"identifier":"u2","displayValue":"Alice"}]}],"requirementIds":["R-1"],"url":"javascript:alert(1)"}),
        ),
        record(
            "bug",
            "no-responsible",
            json!({"creator":"Reporter","owner":"Handler","createdAt":"2026-11-07T00:00:00+08:00"}),
        ),
    ];
    let ids = json!({"identities":[{"name":"Alice","group":"Team","aliases":["git-a","git-b"]}]});
    let roster = json!({"members":[{"name":"git-a","roles":["code"]}]});
    let result = run(&records, ActivityRole::Code, roster.clone(), ids.clone(), permissions());
    assert_eq!(result["events"].as_array().expect("events").len(), 1);
    assert_eq!(result["events"][0]["actor"], "Alice");
    assert_eq!(result["events"][0]["date"], "2026-11-07");
    assert_eq!(result["events"][0]["requirements"].as_array().expect("requirements").len(), 2);
    assert_eq!(result["events"][0]["repositoryUrl"], "https://codeup.aliyun.com/org/repo");
    assert_eq!(result["bugs"][0]["responsibles"], json!(["Alice"]));
    assert_eq!(result["bugs"][1]["responsibles"], json!(["待归属"]));
    assert!(result["bugs"][0]["url"].is_null());
    assert_eq!(result["members"].as_array().expect("members").len(), 1);
    assert_eq!(result["members"][0]["inRoster"], true);
    let hidden = run(&records, ActivityRole::Code, roster, ids, ActivityPermissions::default());
    assert_eq!(hidden["bugs"], json!([]));
    assert_eq!(hidden["events"][0]["requirements"], json!([]));
    assert!(!hidden.to_string().contains("first"));
}
#[test]
fn absence_needs_roster_coverage_and_usable_timestamps_for_all_role_metrics() {
    let records = vec![
        record(
            "requirement",
            "delivery",
            json!({"creator":"old-creator","owner":"delivery-only","status":"已发布","createdAt":"2026-10-01T00:00:00+08:00","statusChangedAt":"2026-11-06T00:00:00+08:00"}),
        ),
        record("requirement", "missing-time", json!({"creator":"undated"})),
        record(
            "requirement",
            "future",
            json!({"creator":"future-person","createdAt":"2026-12-01T00:00:00+08:00"}),
        ),
    ];
    let roster = json!({"members":[{"name":"no-records","roles":["product"]},{"name":"undated","roles":["product"]},{"name":"disabled","roles":["product"],"enabled":false}]});
    let result = run(
        &records,
        ActivityRole::Product,
        roster.clone(),
        json!({"identities":[]}),
        permissions(),
    );
    let members: BTreeStatuses = result["members"]
        .as_array()
        .expect("members")
        .iter()
        .map(|m| {
            (
                m["name"].as_str().expect("name").to_owned(),
                m["status"].as_str().expect("status").to_owned(),
            )
        })
        .collect();
    assert_eq!(members["no-records"], "noRecords");
    assert_eq!(members["undated"], "unverified");
    assert_eq!(members["old-creator"], "unverified");
    assert_eq!(members["delivery-only"], "active");
    assert!(!members.contains_key("disabled"));
    assert!(!members.contains_key("future-person"));
    let incomplete = build_activity(
        &records,
        &source(),
        &ActivityQuery { days: Some(30), role: ActivityRole::Product },
        &AuthorIdentities { identities: vec![] },
        &serde_json::from_value(roster).expect("roster"),
        permissions(),
        CollectionWindow {
            author_groups: vec![],
            now: at("2026-11-07T21:00:00+08:00"),
            collected_since: Some(at("2026-11-06T00:00:00+08:00")),
            testcases_collected: true,
        },
    )
    .expect("incomplete");
    assert_eq!(incomplete["coverage"]["complete"], false);
    assert!(incomplete["members"]
        .as_array()
        .expect("members")
        .iter()
        .all(|m| m["status"] != "noRecords"));
}
type BTreeStatuses = std::collections::BTreeMap<String, String>;
#[test]
fn validates_custom_windows_and_roster_without_default_people() {
    for days in [0, 366] {
        assert!(ActivityQuery { days: Some(days), ..Default::default() }.validate().is_err());
    }
    assert!(ActivityMembers { members: vec![] }
        .normalize()
        .expect("empty config")
        .members
        .is_empty());
    for value in [
        json!({"members":[{"name":"A","roles":[]}]}),
        json!({"members":[{"name":" A ","roles":["code"]},{"name":"A","roles":["test"]}]}),
    ] {
        assert!(serde_json::from_value::<ActivityMembers>(value)
            .expect("config")
            .normalize()
            .is_err());
    }
}

#[test]
fn workitem_links_use_source_identifiers_and_keep_original_urls() {
    let records = vec![
        record(
            "requirement",
            "r",
            json!({"creator":"author","createdAt":"2026-11-03T00:00:00+08:00"}),
        ),
        record(
            "requirement",
            "original",
            json!({"creator":"author","createdAt":"2026-11-03T00:00:00+08:00","url":"https://example.com/original"}),
        ),
        record(
            "requirement",
            "../bad",
            json!({"creator":"author","createdAt":"2026-11-03T00:00:00+08:00"}),
        ),
    ];
    let mut project = source();
    project["cloud_project_id"] = json!("project-id");
    let result = build_activity(
        &records,
        &project,
        &ActivityQuery { days: Some(7), role: ActivityRole::Product },
        &AuthorIdentities { identities: vec![] },
        &ActivityMembers::default(),
        permissions(),
        CollectionWindow {
            author_groups: vec![],
            now: at("2026-11-07T21:00:00+08:00"),
            collected_since: None,
            testcases_collected: false,
        },
    )
    .expect("activity");
    let events = result["events"].as_array().expect("events");
    assert_eq!(
        events.iter().find(|e| e["externalId"] == "r").expect("request")["url"],
        "https://devops.aliyun.com/projex/project/project-id/req/r"
    );
    assert_eq!(
        events.iter().find(|e| e["externalId"] == "original").expect("original")["url"],
        "https://example.com/original"
    );
    assert!(events.iter().find(|e| e["externalId"] == "../bad").expect("invalid")["url"].is_null());
}

#[test]
fn unknown_git_authors_and_unmapped_accounts_keep_their_commits_and_lines() {
    let records = vec![
        record(
            "commit",
            "r:a",
            json!({"repoId":"r","sha":"a","author":"作者未知","authoredAt":"2026-11-04T09:00:00+08:00","additions":9,"deletions":6}),
        ),
        record(
            "commit",
            "r:b",
            json!({"repoId":"r","sha":"b","author":"unmapped-account","authoredAt":"2026-11-04T10:00:00+08:00","additions":3,"deletions":1}),
        ),
    ];
    let data = run(
        &records,
        ActivityRole::Code,
        json!({"members":[]}),
        json!({"identities":[]}),
        permissions(),
    );
    assert_eq!(data["events"].as_array().expect("events").len(), 2);
    assert_eq!(data["coverage"]["unattributedEvents"], 1);
    let members = data["members"].as_array().expect("members");
    assert!(members.iter().any(|p| p["name"] == "" && p["attribution"] == "unknownAuthor"));
    assert!(members
        .iter()
        .any(|p| p["name"] == "unmapped-account" && p["attribution"] == "unmappedAccount"));
    assert_eq!(data["events"][0]["additions"], 9);
    let mut policy = shepherd_yunxiao::domain::activity_review::ActivityReviewPolicy::default();
    assert!(policy.validate().is_ok());
    policy.code.min_active_days = 8;
    assert!(policy.validate().is_err());
}

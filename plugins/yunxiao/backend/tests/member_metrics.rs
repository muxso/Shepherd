use chrono::DateTime;
use serde_json::{json, Value};
use shepherd_yunxiao::domain::{build_member_metrics, MemberMetricsQuery, Record};
fn record(id: &str, kind: &str, data: Value) -> Record {
    Record { external_id: id.into(), kind: kind.into(), data }
}
fn req(id: &str, status: &str, changed: &str, due: &str) -> Record {
    record(
        id,
        "requirement",
        json!({"serial":format!("REQ-{id}"),"title":"shared requirement","status":status,"createdAt":"2026-10-01T00:00:00+08:00","statusChangedAt":changed,"due":due,"owner":"甲","group":"研发组","customFieldValues":[{"fieldName":"开发者","values":[{"displayValue":"乙"}]}]}),
    )
}
fn query() -> MemberMetricsQuery {
    MemberMetricsQuery { month: Some("2026-10".into()), requirement_target: Some(10.0) }
}
fn now() -> chrono::DateTime<chrono::FixedOffset> {
    DateTime::parse_from_rfc3339("2026-11-02T10:00:00+08:00").expect("clock")
}
#[test]
fn due_cohorts_shared_participation_and_missing_time() {
    let records = vec![
        req("on", "已发布", "2026-10-15T23:59:59+08:00", "2026-10-15"),
        req("late", "已发布", "2026-10-16T00:00:00+08:00", "2026-10-15"),
        req("pending", "开发中", "2026-10-02T00:00:00+08:00", "2026-10-15"),
        req("unknown", "测试中", "2026-11-01T00:00:00+08:00", "2026-10-15"),
        req("missing", "测试中", "", "2026-10-15"),
        req("cancel", "已取消", "2026-10-02T00:00:00+08:00", "2026-10-15"),
        req("next", "开发中", "2026-10-02T00:00:00+08:00", "2026-11-01"),
    ];
    let data = build_member_metrics(&records, &query(), now()).expect("metrics");
    assert_eq!(data.members.len(), 2);
    for p in data.members {
        assert_eq!(p.new_requirements, 7);
        assert_eq!(p.delivered_requirements, 2);
        assert_eq!(p.due_requirements, 5);
        assert_eq!(
            (p.on_time, p.delivered_late, p.overdue_at_end, p.unknown_delivery),
            (1, 1, 1, 2)
        );
        assert_eq!(p.delay_rate, Some(66.7));
        assert_eq!(p.bug_rate, None);
        assert_eq!(p.radar[0], Some(20.0));
        assert_eq!(p.radar[1], None);
        assert_eq!(
            p.due_requirements,
            p.on_time + p.delivered_late + p.overdue_at_end + p.unknown_delivery
        );
    }
}
#[test]
fn bug_links_are_exact_deduplicated_and_not_assigned_to_bug_owner() {
    let records = vec![
        req("a", "已发布", "2026-10-12T00:00:00+08:00", "2026-10-15"),
        record(
            "b1",
            "bug",
            json!({"createdAt":"2026-10-04T00:00:00+08:00","status":"已解决","owner":"处理人","parentId":"a","requirementIds":["a","REQ-a"]}),
        ),
        record(
            "b2",
            "bug",
            json!({"createdAt":"2026-10-05T00:00:00+08:00","status":"待处理","requirementIds":["REQ-a"]}),
        ),
        record(
            "b3",
            "bug",
            json!({"createdAt":"2026-10-05T00:00:00+08:00","status":"重复缺陷","parentId":"a"}),
        ),
        record(
            "old",
            "bug",
            json!({"createdAt":"2026-09-30T00:00:00+08:00","status":"待处理","parentId":"a"}),
        ),
    ];
    let data = build_member_metrics(&records, &query(), now()).expect("metrics");
    assert_eq!(data.new_valid_bugs, 2);
    assert_eq!(data.linked_valid_bugs, 2);
    for p in data.members {
        assert_ne!(p.name, "处理人");
        assert_eq!(p.linked_bugs, 2);
        assert_eq!(p.bug_affected_requirements, 1);
        assert_eq!(p.bug_rate, Some(100.0));
        assert_eq!(p.radar, [Some(10.0), Some(0.0), Some(100.0)]);
    }
}
#[test]
fn shanghai_boundaries_defaults_and_target_validation() {
    let mut r = req("a", "已发布", "2026-10-31T16:00:00Z", "2026-10-31");
    r.data["createdAt"] = json!("2026-10-01T00:00:00+08:00");
    let data = build_member_metrics(&[r], &query(), now()).expect("metrics");
    assert_eq!(data.members[0].delivered_requirements, 0);
    assert_eq!(data.members[0].overdue_at_end, 1);
    let january = DateTime::parse_from_rfc3339("2027-01-01T00:01:00+08:00").expect("clock");
    assert_eq!(
        build_member_metrics(&[], &MemberMetricsQuery::default(), january).expect("default").month,
        "2026-12"
    );
    for month in ["2026-13", "2026-1", "2026-00", "0000-01", "2027-01"] {
        let q = MemberMetricsQuery { month: Some(month.into()), ..query() };
        assert!(build_member_metrics(&[], &q, now()).is_err());
    }
    for target in [0.0, -1.0, f64::NAN, f64::INFINITY, 10001.0] {
        assert!(MemberMetricsQuery { requirement_target: Some(target), ..query() }
            .validate()
            .is_err());
    }
}
#[test]
fn no_denominator_or_unlinked_data_is_not_perfect_quality() {
    let data = build_member_metrics(
        &[req("a", "开发中", "2026-10-02T00:00:00+08:00", "")],
        &query(),
        now(),
    )
    .expect("metrics");
    assert_eq!(data.members[0].radar, [Some(0.0), None, None]);
    assert_eq!(data.requirements_missing_due, 1);
    let records = vec![
        req("a", "已发布", "2026-10-10T00:00:00+08:00", "2026-10-10"),
        record("b", "bug", json!({"status":"待处理","createdAt":"2026-10-01T00:00:00+08:00"})),
    ];
    assert_eq!(
        build_member_metrics(&records, &query(), now()).expect("metrics").members[0].bug_rate,
        None
    );
}
#[test]
fn current_month_only_counts_deadlines_that_have_passed() {
    let clock = DateTime::parse_from_rfc3339("2026-10-15T12:00:00+08:00").expect("clock");
    let records = vec![
        req("yesterday", "开发中", "2026-10-02T00:00:00+08:00", "2026-10-14"),
        req("today", "开发中", "2026-10-02T00:00:00+08:00", "2026-10-15"),
    ];
    let data = build_member_metrics(&records, &query(), clock).expect("metrics");
    assert_eq!(data.members[0].due_requirements, 1);
    assert_eq!(data.members[0].delay_rate, Some(100.0));
}

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
        json!({"serial":format!("REQ-{id}"),"title":"shared requirement","status":status,"createdAt":"2026-10-01T00:00:00+08:00","statusChangedAt":changed,"due":due,"owner":"甲","group":"研发组","customFieldValues":[{"fieldName":"开发者","fieldFormat":"multiUser","values":[{"identifier":"user-b","displayValue":"乙"}]}]}),
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
    for p in data.members.into_iter().filter(|p| p.name == "甲" || p.name == "乙") {
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
        "2027-01"
    );
    for month in ["2026-13", "2026-1", "2026- 1", "2026-+1", "2026-00", "0000-01", "2027-01"] {
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

#[test]
fn free_text_roles_are_never_promoted_to_people() {
    let mut requirement = req("typed", "已发布", "2026-10-10T00:00:00+08:00", "2026-10-11");
    requirement.data["customFieldValues"] = json!([
        {"fieldName":"需求提出人","fieldFormat":"text","values":[{"identifier":"cxl","displayValue":"客户名称与很长的需求说明"}]},
        {"fieldName":"开发者","fieldFormat":"text","values":[{"identifier":"text-is-not-id","displayValue":"看似姓名"}]},
        {"fieldName":"协作人","values":[{"identifier":"unknown-type","displayValue":"缺少类型"}]},
        {"fieldName":"测试人员","fieldFormat":"user","values":[{"identifier":"tester-id","displayValue":"测试甲"}]},
        {"fieldName":"开发者","fieldFormat":"multiUser","values":[{"identifier":"dev-id","displayValue":"开发乙"},{"displayValue":"缺少标识"},{"identifier":"AK-rule","displayValue":"自动规则"}]}
    ]);
    let data = build_member_metrics(&[requirement], &query(), now()).expect("metrics");
    let names: Vec<_> = data.members.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.len(), 3);
    for name in ["甲", "测试甲", "开发乙"] {
        assert!(names.contains(&name));
    }
    for name in ["客户名称与很长的需求说明", "看似姓名", "缺少类型", "缺少标识", "自动规则"]
    {
        assert!(!names.contains(&name));
    }
}

fn quality_bug(
    id: &str,
    created: &str,
    responsible: &str,
    transitions: Value,
    events: Value,
    resolutions: Value,
) -> Record {
    record(
        id,
        "bug",
        json!({"serial":id,"createdAt":created,"status":"已验证","owner":"当前测试处理人","creator":"发现人","parentId":"never-published-requirement","customFieldValues":[
        {"fieldName":"缺陷责任人","fieldFormat":"user","values":[{"identifier":"developer","displayValue":responsible}]},
        {"fieldName":"开发者","fieldFormat":"multiUser","values":[{"identifier":"fixer","displayValue":"本次修复人"}]},
        {"fieldName":"测试人员","fieldFormat":"user","values":[{"identifier":"tester","displayValue":"测试人"}]},
        {"fieldName":"缺陷类型","fieldFormat":"list","values":[{"displayValue":"程序逻辑"}]}
    ],"reopenHistory":{"schemaVersion":2,"complete":true,"checkedAt":"2026-11-02T00:00:00+08:00","transitions":transitions,"events":events,"resolutions":resolutions}}),
    )
}
#[test]
fn legacy_bugs_and_unpublished_unlinked_requirements_follow_bug_responsibility() {
    let t = json!({"id":"reopen","at":"2026-10-19T17:00:00+08:00","from":"已解决","to":"重新打开"});
    let e = json!({"id":"reopen","at":"2026-10-19T17:00:00+08:00"});
    let old = quality_bug(
        "legacy",
        "2026-07-17T00:00:00+08:00",
        "责任开发",
        json!([t, t]),
        json!([e, e]),
        json!([]),
    );
    let data = build_member_metrics(&[old], &query(), now()).expect("metrics");
    assert_eq!(data.members.len(), 1);
    let p = &data.members[0];
    assert_eq!(p.name, "责任开发");
    assert_eq!(
        (p.delivered_requirements, p.linked_bugs, p.processed_bugs, p.reopen_count),
        (0, 0, 1, 1)
    );
    assert_eq!(p.reopen_rate, Some(100.0));
    assert_eq!(p.bugs[0].bug_type, "程序逻辑");
    assert!(p.bugs[0].developers.contains("本次修复人"));
    assert!(p.bugs[0].testers.contains("测试人"));
    assert_eq!(p.bugs[0].history.len(), 1);
    assert_eq!(data.bug_quality.reopen_count, 1);
}
#[test]
fn monthly_reopen_ratio_uses_unique_processed_bugs_and_counts_events_separately() {
    let t1 = json!({"id":"1","at":"2026-10-15T00:00:00+08:00","from":"已解决","to":"重新打开"});
    let t2 = json!({"id":"2","at":"2026-10-31T15:59:59Z","from":"已解决","to":"修复中"});
    let t3 = json!({"id":"3","at":"2026-10-31T16:00:00Z","from":"已解决","to":"重新打开"});
    let a = quality_bug(
        "a",
        "2026-07-01T00:00:00+08:00",
        "甲",
        json!([t1, t2, t3]),
        json!([
        {"id":"1","at":"2026-10-15T00:00:00+08:00"},{"id":"2","at":"2026-10-31T15:59:59Z"},{"id":"3","at":"2026-10-31T16:00:00Z"}]),
        json!([]),
    );
    let b = quality_bug(
        "b",
        "2026-09-01T00:00:00+08:00",
        "甲",
        json!([{"id":"r","at":"2026-10-02T00:00:00+08:00","from":"待处理","to":"已解决"}]),
        json!([]),
        json!([{"id":"r","at":"2026-10-02T00:00:00+08:00"}]),
    );
    let new =
        quality_bug("new", "2026-10-02T00:00:00+08:00", "甲", json!([]), json!([]), json!([]));
    let data = build_member_metrics(&[a, b, new], &query(), now()).expect("metrics");
    let p = &data.members[0];
    assert_eq!((p.processed_bugs, p.reopened_bugs, p.reopen_count, p.quality_bugs), (2, 1, 2, 3));
    assert_eq!(p.reopen_rate, Some(50.0));
    assert_eq!(data.bug_quality.rows.len(), 3);
}
#[test]
fn unknown_history_and_role_fields_are_never_guessed() {
    let mut b = quality_bug(
        "legacy",
        "2026-07-01T00:00:00+08:00",
        "自由文本",
        json!([]),
        json!([]),
        json!([]),
    );
    b.data["customFieldValues"][0]["fieldFormat"] = json!("text");
    b.data["reopenHistory"]["schemaVersion"] = json!(1);
    let data = build_member_metrics(&[b], &query(), now()).expect("metrics");
    assert_eq!(data.members[0].name, "待归属");
    assert_eq!(data.members[0].reopen_rate, None);
    assert_eq!(data.members[0].reopen_unknown_bugs, 1);
    assert_eq!(data.bug_quality.unknown_source_bugs, 1);
    assert_eq!(data.bug_quality.reopen_rate, None);
}

#[test]
fn current_month_default_and_comparisons_use_matching_shanghai_cutoffs() {
    let clock = DateTime::parse_from_rfc3339("2026-11-04T12:34:00+08:00").expect("clock");
    let data = build_member_metrics(&[], &MemberMetricsQuery::default(), clock).expect("metrics");
    assert_eq!(data.month, "2026-11");
    assert_eq!(data.end_exclusive, "2026-11-04T12:34:00+08:00");
    let comparison = data.comparisons.expect("comparison");
    assert!(comparison.same_elapsed_time);
    assert_eq!(
        comparison.previous_month.expect("month").end_exclusive,
        "2026-10-04T12:34:00+08:00"
    );
    assert_eq!(comparison.previous_year.expect("year").end_exclusive, "2025-11-04T12:34:00+08:00");
    let data = build_member_metrics(&[], &query(), clock).expect("past month");
    let comparison = data.comparisons.expect("comparison");
    assert!(!comparison.same_elapsed_time);
    assert_eq!(
        comparison.previous_month.expect("month").end_exclusive,
        "2026-10-01T00:00:00+08:00"
    );
    assert_eq!(comparison.previous_year.expect("year").end_exclusive, "2025-11-01T00:00:00+08:00");
}

#[test]
fn comparison_cutoffs_do_not_overflow_short_months_or_leap_years() {
    for (clock, prior_month_end, prior_year_end) in [
        ("2026-03-31T12:00:00+08:00", "2026-03-01T00:00:00+08:00", "2025-03-31T12:00:00+08:00"),
        ("2024-02-29T12:00:00+08:00", "2024-01-29T12:00:00+08:00", "2023-03-01T00:00:00+08:00"),
        ("2027-01-04T12:00:00+08:00", "2026-12-04T12:00:00+08:00", "2026-01-04T12:00:00+08:00"),
    ] {
        let clock = DateTime::parse_from_rfc3339(clock).expect("clock");
        let comparison = build_member_metrics(&[], &MemberMetricsQuery::default(), clock)
            .expect("metrics")
            .comparisons
            .expect("comparison");
        assert_eq!(comparison.previous_month.expect("month").end_exclusive, prior_month_end);
        assert_eq!(comparison.previous_year.expect("year").end_exclusive, prior_year_end);
    }
}

#[test]
fn comparison_counts_exclude_deliveries_after_the_aligned_cutoff() {
    let records = [
        req("before", "已发布", "2026-10-03T12:00:00+08:00", "2026-10-03"),
        req("after", "已发布", "2026-10-05T12:00:00+08:00", "2026-10-05"),
    ];
    let clock = DateTime::parse_from_rfc3339("2026-11-04T12:00:00+08:00").expect("clock");
    let data =
        build_member_metrics(&records, &MemberMetricsQuery::default(), clock).expect("metrics");
    let prior = data.comparisons.expect("comparison").previous_month.expect("prior");
    assert_eq!(prior.members.len(), 2);
    assert!(prior.members.iter().all(|m| m.delivered_requirements == 1));
    let full = build_member_metrics(&records, &query(), clock).expect("full");
    assert!(full.members.iter().all(|m| m.delivered_requirements == 2));
}

#[test]
fn past_partial_comparison_requires_bug_history_through_its_actual_cutoff() {
    let mut bug = quality_bug(
        "partial-history",
        "2026-10-02T00:00:00+08:00",
        "责任开发",
        json!([]),
        json!([]),
        json!([]),
    );
    bug.data["reopenHistory"]["checkedAt"] = json!("2026-10-02T00:00:00+08:00");
    let clock = DateTime::parse_from_rfc3339("2026-11-04T12:00:00+08:00").expect("clock");
    let data =
        build_member_metrics(&[bug], &MemberMetricsQuery::default(), clock).expect("metrics");
    let prior = data.comparisons.expect("comparison").previous_month.expect("prior");
    assert_eq!(prior.members[0].reopen_unknown_bugs, 1);
    assert_eq!(prior.members[0].reopen_rate, None);
}

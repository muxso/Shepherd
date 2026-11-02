#![cfg(feature = "pg")]
use serde_json::json;
use shepherd_yunxiao::{
    adapters::pg::PgStore,
    domain::{
        CompleteRun, ListQuery, NewSource, ProductivityMetric, Record, RequirementOutputQuery,
    },
    ports::Store,
};
fn completion() -> CompleteRun {
    CompleteRun {
        counts: [
            ("requirement".into(), 1),
            ("bug".into(), 0),
            ("sprint".into(), 0),
            ("commit".into(), 0),
        ]
        .into(),
        code_since: "2026-10-21T00:00:00+08:00".into(),
        code_until: "2026-10-28T00:00:00+08:00".into(),
    }
}
#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn atomic_snapshot_idempotency_history_and_failures() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("isolated test database"),
    )
    .await
    .expect("connect");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("migrate");
    let project = uuid::Uuid::new_v4().to_string();
    let store = PgStore(pool.clone());
    let source = store
        .create_source(NewSource {
            project_id: project.clone(),
            cloud_org_id: "test".into(),
            cloud_project_id: project,
            name: "test".into(),
        })
        .await
        .expect("source");
    let source = source["id"].as_str().expect("id");
    let r = Record {
        kind: "requirement".into(),
        external_id: "req".into(),
        data: json!({"serial":"R-1","title":"Test","status":"开发中","priority":"高","due":"2026-01-01"}),
    };
    let run = store.start(source).await.expect("start");
    let run = run["id"].as_str().expect("id");
    assert!(store.start(source).await.is_err());
    let mut wrong = completion();
    wrong.counts.insert("requirement".into(), 2);
    store.stage(run, vec![r.clone()]).await.expect("stage");
    assert!(store.complete(run, wrong).await.is_err());
    assert_eq!(store.dashboard(source, 7).await.expect("dashboard")["totals"]["requirements"], 0);
    store.complete(run, completion()).await.expect("complete");
    assert_eq!(store.dashboard(source, 7).await.expect("dashboard")["totals"]["requirements"], 1);
    for status in ["开发中", "测试中"] {
        let run = store.start(source).await.expect("start");
        let run = run["id"].as_str().expect("id");
        let mut r = r.clone();
        r.data["status"] = json!(status);
        store.stage(run, vec![r]).await.expect("stage");
        store.complete(run, completion()).await.expect("complete");
    }
    let histories =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM ms_devops_history WHERE source_id=$1")
            .bind(source)
            .fetch_one(&pool)
            .await
            .expect("history");
    assert_eq!(histories, 1);
    let run = store.start(source).await.expect("start");
    store.fail(run["id"].as_str().expect("id"), "test failure").await.expect("fail");
    assert_eq!(store.dashboard(source, 7).await.expect("dashboard")["totals"]["requirements"], 1);
}

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn snapshot_stays_in_plugin_database_without_native_tables() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("plugin test database"),
    )
    .await
    .expect("connect");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("plugin migrations");
    let store = PgStore(pool.clone());
    let source = store
        .create_source(NewSource {
            project_id: "opaque-reference-without-native-project".into(),
            cloud_org_id: "test".into(),
            cloud_project_id: uuid::Uuid::new_v4().to_string(),
            name: "isolated".into(),
        })
        .await
        .expect("source does not require a native project");
    let source = source["id"].as_str().expect("source id");
    let mut complete = completion();
    complete.counts.insert("testcase".into(), 1);
    for description in ["original", "changed"] {
        let run = store.start(source).await.expect("start");
        let run = run["id"].as_str().expect("run id");
        store.stage(run, vec![
            Record { kind: "requirement".into(), external_id: "req".into(), data: json!({"title":"requirement","raw":{"description":description}}) },
            Record { kind: "testcase".into(), external_id: "case".into(), data: json!({"title":"case","steps":[{"step":"action","expected":"result"}]}) },
        ]).await.expect("stage");
        store.complete(run, complete.clone()).await.expect("complete");
    }
    let body: String = sqlx::query_scalar("SELECT payload #>> '{raw,description}' FROM ms_devops_entity WHERE source_id=$1 AND kind='requirement'")
        .bind(source).fetch_one(&pool).await.expect("plugin body");
    assert_eq!(body, "changed");
    let native_exists: bool = sqlx::query_scalar("SELECT to_regclass('ms_project') IS NOT NULL OR to_regclass('ms_requirement') IS NOT NULL OR to_regclass('ms_functional_case') IS NOT NULL OR to_regclass('ms_api_module') IS NOT NULL")
        .fetch_one(&pool).await.expect("native table check");
    assert!(!native_exists, "plugin migrations must not install native tables");
}

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn productivity_uses_source_dates_creators_and_isolated_project_facts() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("isolated test database"),
    )
    .await
    .expect("connect");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("migrate");
    let store = PgStore(pool.clone());
    let source = store
        .create_source(NewSource {
            project_id: "output-test".into(),
            cloud_org_id: "test".into(),
            cloud_project_id: uuid::Uuid::new_v4().to_string(),
            name: "Output test".into(),
        })
        .await
        .expect("source");
    let id = source["id"].as_str().expect("id");
    let today = sqlx::query_scalar::<_, String>(
        "SELECT to_char((now() AT TIME ZONE 'Asia/Shanghai')::date,'YYYY-MM-DD')",
    )
    .fetch_one(&pool)
    .await
    .expect("today");
    let previous = sqlx::query_scalar::<_, String>(
        "SELECT to_char((now() AT TIME ZONE 'Asia/Shanghai')::date-7,'YYYY-MM-DD')",
    )
    .fetch_one(&pool)
    .await
    .expect("previous");
    let records = [
        (
            "requirement",
            "new",
            json!({"title":"交付具体需求","serial":"REQ-42","creator":"产品作者","owner":"当前负责人","createdAt":format!("{today}T00:00:00+08:00"),"status":"已发布","statusChangedAt":format!("{today}T12:00:00+08:00")}),
        ),
        (
            "requirement",
            "previous",
            json!({"creator":"产品作者","createdAt":format!("{previous}T23:59:59+08:00")}),
        ),
        ("requirement", "missing-time", json!({"creator":"产品作者"})),
        (
            "bug",
            "missing-creator",
            json!({"parentId":"new","owner":"不能算创建人","status":"修复中","createdAt":format!("{today}T01:00:00+08:00")}),
        ),
        (
            "testcase",
            "case",
            json!({"creator":"测试作者","createdAt":format!("{today}T01:00:00+08:00")}),
        ),
        (
            "commit",
            "linked",
            json!({"repoId":"repo-a","repo":"服务端","author":"研发作者","authoredAt":format!("{today}T01:00:00+08:00"),"requirementIds":["new"],"additions":12,"deletions":3}),
        ),
        (
            "commit",
            "unknown",
            json!({"repoId":"repo-b","repo":"前端","author":"研发作者","authoredAt":format!("{today}T02:00:00+08:00"),"requirementIds":["REQ-42"]}),
        ),
        (
            "commit",
            "unlinked",
            json!({"author":"其他作者","authoredAt":format!("{today}T03:00:00+08:00"),"requirementIds":[],"additions":999,"deletions":999}),
        ),
        (
            "testcase",
            "inactive",
            json!({"creator":"删除作者","createdAt":format!("{today}T01:00:00+08:00")}),
        ),
        (
            "testcase",
            "future",
            json!({"creator":"未来作者","createdAt":"2999-01-01T00:00:00+08:00"}),
        ),
    ];
    for (kind, external, data) in records {
        sqlx::query(
            "INSERT INTO ms_devops_entity(source_id,kind,external_id,payload) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(kind)
        .bind(external)
        .bind(data)
        .execute(&pool)
        .await
        .expect("fact");
    }
    // Inactive snapshots never contribute, regardless of their dates.
    sqlx::query(
        "UPDATE ms_devops_entity SET active=false WHERE source_id=$1 AND external_id='inactive'",
    )
    .bind(id)
    .execute(&pool)
    .await
    .expect("inactive");
    let dashboard = store.dashboard(id, 7).await.expect("dashboard");
    let metrics = &dashboard["productivity"]["metrics"];
    assert_eq!(dashboard["bugStatistics"]["total"], 1);
    assert_eq!(dashboard["bugStatistics"]["linked"], 1);
    assert_eq!(dashboard["bugStatistics"]["unlinked"], 0);
    let output = store
        .requirement_output(RequirementOutputQuery {
            source_id: Some(id.into()),
            reference: "REQ-42".into(),
            ..Default::default()
        })
        .await
        .expect("requirement output");
    assert_eq!(output["requirement"]["creator"], "产品作者");
    assert_eq!(output["output"]["bugTotal"], 1);
    assert_eq!(output["output"]["commitTotal"], 2);
    assert_eq!(output["output"]["repositories"].as_array().expect("repositories").len(), 2);
    assert!(output["output"]["bugs"][0].get("raw").is_none());
    let project_output = store
        .requirement_output(RequirementOutputQuery {
            project_id: Some("output-test".into()),
            reference: "new".into(),
            ..Default::default()
        })
        .await
        .expect("project reference");
    assert_eq!(project_output["output"]["commitTotal"], 2);
    assert!(store
        .requirement_output(RequirementOutputQuery {
            project_id: Some("foreign-project".into()),
            reference: "REQ-42".into(),
            ..Default::default()
        })
        .await
        .is_err());
    assert!(store
        .requirement_output(RequirementOutputQuery {
            reference: "REQ-42".into(),
            ..Default::default()
        })
        .await
        .is_err());

    assert_eq!(metrics["requirements"]["current"], 1);
    assert_eq!(metrics["requirements"]["previous"], 1);
    assert_eq!(metrics["requirements"]["missingTime"], 1);
    assert_eq!(metrics["requirements"]["contributors"][0]["name"], "产品作者");
    assert_eq!(metrics["delivered"]["contributors"][0]["name"], "当前负责人");
    assert_eq!(metrics["bugs"]["unattributed"], 1);
    assert_eq!(metrics["bugs"]["contributors"][0]["name"], "待归属");
    assert_eq!(metrics["testcases"]["current"], 1);
    assert_eq!(metrics["commits"]["current"], 3);
    assert_eq!(metrics["linkedCommits"]["current"], 2);
    assert_eq!(metrics["additions"]["current"], 12);
    assert_eq!(metrics["deletions"]["current"], 3);
    assert_eq!(metrics["unknownStats"]["current"], 1);
    let trend = metrics["requirements"]["trend"].as_array().expect("trend");
    assert_eq!(trend.len(), 7);
    assert_eq!(trend.last().expect("today")["date"], today);
    assert_eq!(trend.last().expect("today")["value"], 1);
    assert!(trend[..6].iter().all(|point| point["value"] == 0));
    for (metric, kind, expected) in [
        (ProductivityMetric::Requirements, "requirement", 1),
        (ProductivityMetric::Delivered, "requirement", 1),
        (ProductivityMetric::Testcases, "testcase", 1),
        (ProductivityMetric::Bugs, "bug", 1),
        (ProductivityMetric::LinkedCommits, "commit", 2),
    ] {
        let rows = store
            .records(
                id,
                ListQuery {
                    metric: Some(metric),
                    date: Some(today.clone()),
                    kind: Some(kind.into()),
                    page_size: Some(1),
                    ..Default::default()
                },
            )
            .await
            .expect("daily records");
        assert_eq!(rows["total"], expected);
        assert_eq!(rows["items"].as_array().expect("items").len(), 1);
        assert!(rows["items"][0].get("raw").is_none());
        if kind == "commit" {
            assert_eq!(rows["items"][0]["requirements"][0]["externalId"], "new");
            assert_eq!(rows["items"][0]["requirements"][0]["title"], "交付具体需求");
        }
    }
    assert!(store
        .records(
            id,
            ListQuery {
                metric: Some(ProductivityMetric::Delivered),
                date: Some("2026-02-30".into()),
                ..Default::default()
            }
        )
        .await
        .is_err());
    assert!(store
        .records(
            id,
            ListQuery {
                metric: Some(ProductivityMetric::Testcases),
                date: Some(today.clone()),
                kind: Some("requirement".into()),
                ..Default::default()
            }
        )
        .await
        .is_err());
    assert!(store
        .records(id, ListQuery { date: Some(today), ..Default::default() })
        .await
        .is_err());
    sqlx::query("DELETE FROM ms_devops_entity WHERE source_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("cleanup");
    sqlx::query("DELETE FROM ms_devops_source WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("cleanup");
}

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn monthly_member_metrics_use_one_source_and_projected_snapshot() {
    use shepherd_yunxiao::domain::MemberMetricsQuery;
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("isolated db"),
    )
    .await
    .expect("connect");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("migrate");
    let store = PgStore(pool);
    let mut sources = Vec::new();
    for n in 0..2 {
        let source = store
            .create_source(NewSource {
                project_id: format!("radar-{n}"),
                cloud_org_id: "test".into(),
                cloud_project_id: uuid::Uuid::new_v4().to_string(),
                name: "radar".into(),
            })
            .await
            .expect("source");
        let id = source["id"].as_str().expect("id").to_owned();
        let run = store.start(&id).await.expect("start");
        let run = run["id"].as_str().expect("id");
        let mut complete = completion();
        complete.counts.insert("bug".into(), if n == 0 { 1 } else { 0 });
        let mut records = vec![Record {
            kind: "requirement".into(),
            external_id: "shared-id".into(),
            data: json!({"serial":"REQ-42","status":"已发布","createdAt":"2026-10-01T00:00:00+08:00","statusChangedAt":"2026-10-15T23:59:59+08:00","due":"2026-10-15","owner":"owner","group":"test-group","raw":{"description":"secret raw text","customFieldValues":[{"fieldName":"开发者","fieldFormat":"multiUser","values":[{"identifier":"developer-id","displayValue":"developer"}]}]}}),
        }];
        if n == 0 {
            records.push(Record {kind:"bug".into(),external_id:"bug".into(),data:json!({"createdAt":"2026-10-10T00:00:00+08:00","status":"已解决","parentId":"shared-id","requirementIds":["REQ-42"],"reopenHistory":{"complete":true,"checkedAt":"2026-11-02T00:00:00+08:00","events":[{"id":"reopen-1","at":"2026-10-20T00:00:00+08:00"}]}})});
        }
        store.stage(run, records).await.expect("stage");
        store.complete(run, complete).await.expect("complete");
        sources.push(id);
    }
    for (n, source) in sources.iter().enumerate() {
        let data = store
            .member_metrics(
                source,
                MemberMetricsQuery {
                    month: Some("2026-10".into()),
                    requirement_target: Some(20.0),
                },
            )
            .await
            .expect("metrics");
        assert_eq!(data["source"]["id"], *source);
        let members = data["metrics"]["members"].as_array().expect("members");
        assert_eq!(members.len(), 2);
        for member in members {
            assert_eq!(member["deliveredRequirements"], 1);
            assert_eq!(member["onTime"], 1);
            assert_eq!(member["delayRate"], 0.0);
            assert_eq!(member["bugAffectedRequirements"], if n == 0 { 1 } else { 0 });
            assert_eq!(member["radar"][0], 5.0);
            assert_eq!(member["reopenCount"], if n == 0 { 1 } else { 0 });
            assert_eq!(member["reopenEvaluatedBugs"], if n == 0 { 1 } else { 0 });
            assert_eq!(member["reopenRate"], if n == 0 { json!(100.0) } else { json!(null) });
        }
        assert!(!data.to_string().contains("secret raw text"));
        assert!(!data.to_string().contains("customFieldValues"));
    }
    assert!(store.member_metrics("missing-source", MemberMetricsQuery::default()).await.is_err());
}

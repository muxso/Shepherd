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
            records.push(Record {kind:"bug".into(),external_id:"bug".into(),data:json!({"createdAt":"2026-10-10T00:00:00+08:00","status":"已解决","parentId":"shared-id","requirementIds":["REQ-42"],"raw":{"customFieldValues":[{"fieldName":"缺陷责任人","fieldFormat":"user","values":[{"identifier":"developer-id","displayValue":"developer"}]}]},"reopenHistory":{"schemaVersion":2,"complete":true,"checkedAt":"2026-11-02T00:00:00+08:00","events":[{"id":"reopen-1","at":"2026-10-20T00:00:00+08:00"}],"resolutions":[],"transitions":[{"id":"reopen-1","at":"2026-10-20T00:00:00+08:00","from":"已解决","to":"重新打开"}]}})});
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
            let responsible = n == 0 && member["name"] == "developer";
            assert_eq!(member["reopenCount"], if responsible { 1 } else { 0 });
            assert_eq!(member["reopenEvaluatedBugs"], if responsible { 1 } else { 0 });
            assert_eq!(member["reopenRate"], if responsible { json!(100.0) } else { json!(null) });
        }
        assert!(!data.to_string().contains("secret raw text"));
        assert!(!data.to_string().contains("customFieldValues"));
    }
    assert!(store.member_metrics("missing-source", MemberMetricsQuery::default()).await.is_err());
}

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn code_authors_use_exact_month_source_and_requirement_visibility() {
    use shepherd_yunxiao::domain::code_authors::CodeAuthorsQuery;
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("test database"),
    )
    .await
    .expect("connect");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("migrate");
    let store = PgStore(pool.clone());
    let source = store
        .create_source(NewSource {
            project_id: "code-test".into(),
            cloud_org_id: "test".into(),
            cloud_project_id: uuid::Uuid::new_v4().to_string(),
            name: "code".into(),
        })
        .await
        .expect("source");
    let id = source["id"].as_str().expect("id");
    for (external, at) in [
        ("before", "2026-10-31T15:59:59Z"),
        ("start", "2026-10-31T16:00:00Z"),
        ("end", "2026-11-30T15:59:59Z"),
        ("after", "2026-11-30T16:00:00Z"),
    ] {
        sqlx::query("INSERT INTO ms_devops_entity(source_id,kind,external_id,payload) VALUES($1,'commit',$2,$3)")
        .bind(id).bind(external).bind(json!({"repoId":42,"sha":external,"authoredAt":at,"author":"甲","requirementIds":["R-1"]})).execute(&pool).await.expect("commit");
    }
    sqlx::query("INSERT INTO ms_devops_entity(source_id,kind,external_id,payload) VALUES($1,'requirement','req-1',$2)").bind(id).bind(json!({"serial":"R-1","title":"源内需求"})).execute(&pool).await.expect("requirement");
    let query = || CodeAuthorsQuery { month: "2026-11".into() };
    let result = store.code_authors(id, query(), true).await.expect("facts");
    let commits = result["commits"].as_array().expect("commits");
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0]["id"], "start");
    assert_eq!(commits[1]["id"], "end");
    assert_eq!(commits[0]["requirements"][0]["title"], "源内需求");
    assert!(commits[0]["additions"].is_null());
    let hidden = store.code_authors(id, query(), false).await.expect("redacted");
    assert_eq!(hidden["commits"][0]["requirements"], json!([]));
    assert!(store.code_authors("missing", query(), true).await.is_err());
    sqlx::query("DELETE FROM ms_devops_entity WHERE source_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("cleanup");
    sqlx::query("DELETE FROM ms_devops_source WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("cleanup source");
}

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn configurable_identities_apply_before_every_code_query_and_survive_sync() {
    use chrono::{DateTime, Duration, FixedOffset};
    use shepherd_yunxiao::domain::{
        author_identities::AuthorIdentities,
        code_authors::CodeAuthorsQuery,
        team_activity::{ActivityMembers, ActivityPermissions, ActivityQuery, ActivityRole},
    };
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("isolated db"),
    )
    .await
    .expect("connect");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("migrate");
    let store = PgStore(pool.clone());
    let mut ids = Vec::new();
    for _ in 0..2 {
        let s = store
            .create_source(NewSource {
                project_id: "identity-test".into(),
                cloud_org_id: "test".into(),
                cloud_project_id: uuid::Uuid::new_v4().to_string(),
                name: "identity test".into(),
            })
            .await
            .expect("source");
        ids.push(s["id"].as_str().expect("id").to_owned());
    }
    let clock: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(now())").fetch_one(&pool).await.expect("clock");
    let now = DateTime::parse_from_rfc3339(clock.as_str().expect("clock string")).expect("time");
    let stamp = (now - Duration::hours(1)).to_rfc3339();
    let records = vec![
        Record {
            kind: "requirement".into(),
            external_id: "r1".into(),
            data: json!({"serial":"REQ-1","title":"first request","creator":"Product creator","owner":"Delivery owner","status":"已发布","createdAt":stamp,"statusChangedAt":stamp,"url":"https://example.com/requests/r1"}),
        },
        Record {
            kind: "requirement".into(),
            external_id: "r2".into(),
            data: json!({"serial":"REQ-2","title":"second request","createdAt":stamp}),
        },
        Record {
            kind: "commit".into(),
            external_id: "repo:a".into(),
            data: json!({"author":"Alice","group":"Team","repoId":"repo","repo":"service","sha":"a","authoredAt":stamp,"requirementIds":["r1"],"additions":10,"deletions":5}),
        },
        Record {
            kind: "commit".into(),
            external_id: "repo:b".into(),
            data: json!({"author":"git-a","group":"待归属","repoId":"repo","repo":"service","sha":"b","authoredAt":stamp,"requirementIds":["r1","REQ-1","r2"],"additions":20,"deletions":2,"url":"https://codeup.aliyun.com/org/service/commit/b"}),
        },
        Record {
            kind: "commit".into(),
            external_id: "repo-2:c".into(),
            data: json!({"author":"git-b","group":"待归属","repoId":"repo-2","repo":"console","sha":"c","authoredAt":stamp,"requirementIds":["r1"]}),
        },
        Record {
            kind: "bug".into(),
            external_id: "bug".into(),
            data: json!({"creator":"Real reporter","owner":"Not the creator","createdAt":stamp,"requirementIds":["r1"],"raw":{"customFieldValues":[{"fieldName":"缺陷责任人","fieldFormat":"user","values":[{"identifier":"alice-id","displayValue":"git-b"}]}]}}),
        },
        Record {
            kind: "testcase".into(),
            external_id: "case".into(),
            data: json!({"creator":"Real case author","owner":"Not the case creator","createdAt":stamp}),
        },
    ];
    for r in &records {
        sqlx::query(
            "INSERT INTO ms_devops_entity(source_id,kind,external_id,payload) VALUES($1,$2,$3,$4)",
        )
        .bind(&ids[0])
        .bind(&r.kind)
        .bind(&r.external_id)
        .bind(&r.data)
        .execute(&pool)
        .await
        .expect("fact");
    }
    sqlx::query("INSERT INTO ms_devops_entity(source_id,kind,external_id,payload) VALUES($1,'commit','repo:b',$2)").bind(&ids[1]).bind(&records[3].data).execute(&pool).await.expect("foreign source");
    for id in &ids {
        sqlx::query("UPDATE ms_devops_source SET last_success_at=now(),code_since=now()-interval '90 days',code_until=now() WHERE id=$1").bind(id).execute(&pool).await.expect("coverage");
    }
    let config:AuthorIdentities=serde_json::from_value(json!({"identities":[{"name":"Alice","aliases":["git-a","git-b"]},{"name":"Bob","group":"Other team","aliases":["git-c"]}]})).expect("config");
    store.set_author_identities(&ids[0], config.clone()).await.expect("save");
    let invalid:AuthorIdentities=serde_json::from_value(json!({"identities":[{"name":"Alice","aliases":["shared"]},{"name":"Bob","aliases":["shared"]}]})).expect("invalid");
    assert!(store.set_author_identities(&ids[0], invalid).await.is_err());
    assert_eq!(
        store.author_identities(&ids[0]).await.expect("config")["identities"]
            .as_array()
            .expect("identities")
            .len(),
        2
    );
    let dashboard = store.dashboard(&ids[0], 7).await.expect("dashboard");
    assert_eq!(dashboard["authors"].as_array().expect("authors").len(), 1);
    assert_eq!(dashboard["authors"][0]["name"], "Alice");
    assert_eq!(dashboard["authors"][0]["group"], "Team");
    assert_eq!(dashboard["authors"][0]["commits"], 3);
    assert_eq!(dashboard["authors"][0]["unknownStats"], 1);
    assert_eq!(dashboard["productivity"]["metrics"]["commits"]["contributors"][0]["name"], "Alice");
    assert_eq!(dashboard["productivity"]["metrics"]["commits"]["contributors"][0]["count"], 3);
    let rows = store
        .records(
            &ids[0],
            ListQuery {
                kind: Some("commit".into()),
                search: Some("Alice".into()),
                group: Some("Team".into()),
                page_size: Some(1),
                ..Default::default()
            },
        )
        .await
        .expect("filtered page");
    assert_eq!(rows["total"], 3);
    assert_eq!(rows["items"].as_array().expect("page").len(), 1);
    let raw = store
        .records(
            &ids[0],
            ListQuery {
                kind: Some("commit".into()),
                search: Some("git-a".into()),
                ..Default::default()
            },
        )
        .await
        .expect("raw search");
    assert_eq!(raw["total"], 1);
    assert_eq!(raw["items"][0]["author"], "Alice");
    assert_eq!(raw["items"][0]["authorRaw"], "git-a");
    let output = store
        .requirement_output(RequirementOutputQuery {
            source_id: Some(ids[0].clone()),
            reference: "REQ-1".into(),
            ..Default::default()
        })
        .await
        .expect("output");
    assert_eq!(output["output"]["developers"].as_array().expect("developers").len(), 1);
    assert_eq!(output["output"]["developers"][0]["commits"], 3);
    let month = now
        .with_timezone(&FixedOffset::east_opt(8 * 3600).expect("tz"))
        .format("%Y-%m")
        .to_string();
    let authors = store
        .code_authors(&ids[0], CodeAuthorsQuery { month: month.clone() }, true)
        .await
        .expect("authors");
    assert!(authors["commits"].as_array().expect("commits").iter().all(|c| c["author"] == "Alice"));
    let foreign = store
        .code_authors(&ids[1], CodeAuthorsQuery { month }, true)
        .await
        .expect("foreign authors");
    assert_eq!(foreign["commits"][0]["author"], "git-a");
    let roster:ActivityMembers=serde_json::from_value(json!({"members":[{"name":"Alice","roles":["code"]},{"name":"No activity","roles":["code","product"]}]})).expect("roster");
    store.set_activity_members(&ids[0], roster).await.expect("roster save");
    let perms = ActivityPermissions { requirements: true, bugs: true, testcases: true };
    let activity =
        store.team_activity(&ids[0], ActivityQuery::default(), perms).await.expect("activity");
    assert_eq!(activity["events"].as_array().expect("events").len(), 3);
    assert_eq!(activity["bugs"][0]["responsibles"], json!(["Alice"]));
    assert!(activity["members"]
        .as_array()
        .expect("members")
        .iter()
        .any(|m| m["name"] == "No activity" && m["status"] == "noRecords"));
    let mut policy = shepherd_yunxiao::domain::activity_review::ActivityReviewPolicy::default();
    assert_eq!(
        store.activity_review_policy(&ids[0]).await.expect("default policy")["code"]
            ["inactiveDays"],
        3
    );
    policy.code.inactive_days = 5;
    store.set_activity_review_policy(&ids[0], policy).await.expect("save policy");
    assert_eq!(
        store.activity_review_policy(&ids[0]).await.expect("saved policy")["code"]["inactiveDays"],
        5
    );
    assert_eq!(
        store.activity_review_policy(&ids[1]).await.expect("isolated policy")["code"]
            ["inactiveDays"],
        3
    );
    assert_eq!(
        store
            .team_activity(&ids[0], ActivityQuery::default(), perms)
            .await
            .expect("policy in activity")["reviewPolicy"]["code"]["inactiveDays"],
        5
    );
    let original: serde_json::Value = sqlx::query_scalar(
        "SELECT payload FROM ms_devops_entity WHERE source_id=$1 AND external_id='repo:b'",
    )
    .bind(&ids[0])
    .fetch_one(&pool)
    .await
    .expect("original");
    assert_eq!(original["author"], "git-a");
    assert_eq!(original["group"], "待归属");
    let run = store.start(&ids[0]).await.expect("sync");
    let run = run["id"].as_str().expect("run");
    store.stage(run, records).await.expect("stage");
    store
        .complete(
            run,
            CompleteRun {
                counts: [
                    ("requirement".into(), 2),
                    ("bug".into(), 1),
                    ("commit".into(), 3),
                    ("testcase".into(), 1),
                    ("sprint".into(), 0),
                ]
                .into(),
                code_since: (now - Duration::days(2)).to_rfc3339(),
                code_until: now.to_rfc3339(),
            },
        )
        .await
        .expect("complete");
    assert_eq!(
        store.author_identities(&ids[0]).await.expect("config retained")["identities"]
            .as_array()
            .expect("identities")
            .len(),
        2
    );
    for role in [ActivityRole::Product, ActivityRole::Test] {
        let data = store
            .team_activity(&ids[0], ActivityQuery { role, ..Default::default() }, perms)
            .await
            .expect("real role events");
        assert!(!data.to_string().contains("Not the creator"));
        assert!(!data.to_string().contains("Not the case creator"));
        let actors: Vec<_> = data["events"]
            .as_array()
            .expect("events")
            .iter()
            .filter_map(|e| e["actor"].as_str())
            .collect();
        if role == ActivityRole::Product {
            assert!(actors.contains(&"Product creator"));
            assert!(actors.contains(&"Delivery owner"));
        } else {
            assert!(actors.contains(&"Real reporter"));
            assert!(actors.contains(&"Real case author"));
        }
    }
    assert!(store.activity_members("missing-source").await.is_err());
    assert!(store.author_identities("missing-source").await.is_err());
    for table in ["ms_devops_history", "ms_devops_entity", "ms_devops_sync_run", "ms_devops_source"]
    {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE {}=ANY($1)",
            if table == "ms_devops_source" { "id" } else { "source_id" }
        ))
        .bind(&ids)
        .execute(&pool)
        .await
        .expect("cleanup");
    }
}

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_YUNXIAO_TEST_DATABASE_URL"]
async fn defect_filters_and_pending_release_risk_use_the_same_source_facts() {
    use shepherd_yunxiao::domain::{
        author_identities::AuthorIdentities,
        bug_statistics::{BugPersonRole, BugStatisticsQuery},
    };
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_YUNXIAO_TEST_DATABASE_URL").expect("valid fixture"),
    )
    .await
    .expect("valid fixture");
    shepherd_yunxiao::adapters::pg::migrate(&pool).await.expect("valid fixture");
    let store = PgStore(pool.clone());
    let source = store
        .create_source(NewSource {
            project_id: "defects".into(),
            cloud_org_id: "test".into(),
            cloud_project_id: uuid::Uuid::new_v4().to_string(),
            name: "defect filters".into(),
        })
        .await
        .expect("valid fixture");
    let id = source["id"].as_str().expect("valid fixture");
    let clock: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(now())").fetch_one(&pool).await.expect("valid fixture");
    let now = chrono::DateTime::parse_from_rfc3339(clock.as_str().expect("valid fixture"))
        .expect("valid fixture");
    let at = (now - chrono::Duration::hours(1)).to_rfc3339();
    let due = (now
        .with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).expect("valid fixture"))
        .date_naive()
        - chrono::Duration::days(1))
    .to_string();
    let records = vec![
        Record {
            kind: "requirement".into(),
            external_id: "ready".into(),
            data: json!({"serial":"R-1","title":"Ready","status":"待发布","priority":"高","due":due,"createdAt":at}),
        },
        Record {
            kind: "requirement".into(),
            external_id: "late".into(),
            data: json!({"title":"Late","status":"开发中","priority":"高","due":due,"createdAt":at}),
        },
        Record {
            kind: "bug".into(),
            external_id: "one".into(),
            data: json!({"title":"One","creator":"Reporter","owner":"Bob","group":"B","createdAt":at,"parentId":"ready","customFieldValues":[{"fieldName":"缺陷责任人","fieldFormat":"user","values":[{"identifier":"a","displayValue":"git-a"}]}]}),
        },
        Record {
            kind: "bug".into(),
            external_id: "two".into(),
            data: json!({"title":"Two","creator":"Reporter","owner":"Bob","group":"B","createdAt":at,"parentId":"EMPTY_VALUE"}),
        },
    ];
    let run = store.start(id).await.expect("valid fixture");
    let run = run["id"].as_str().expect("valid fixture");
    store.stage(run, records).await.expect("valid fixture");
    store
        .complete(
            run,
            CompleteRun {
                counts: [
                    ("requirement".into(), 2),
                    ("bug".into(), 2),
                    ("commit".into(), 0),
                    ("sprint".into(), 0),
                ]
                .into(),
                code_since: (now - chrono::Duration::days(7)).to_rfc3339(),
                code_until: now.to_rfc3339(),
            },
        )
        .await
        .expect("valid fixture");
    let identities: AuthorIdentities = serde_json::from_value(
        json!({"identities":[{"name":"Alice","aliases":["git-a"],"group":"A"}]}),
    )
    .expect("valid fixture");
    store.set_author_identities(id, identities).await.expect("valid fixture");
    let stats = store
        .bug_statistics(
            id,
            BugStatisticsQuery {
                person: Some("Alice".into()),
                group: Some("A".into()),
                ..Default::default()
            },
        )
        .await
        .expect("valid fixture");
    assert_eq!(stats["total"], 1);
    assert_eq!(stats["linked"], 1);
    let discovered = store
        .bug_statistics(
            id,
            BugStatisticsQuery {
                person_role: Some(BugPersonRole::Reporter),
                person: Some("Reporter".into()),
                ..Default::default()
            },
        )
        .await
        .expect("valid fixture");
    assert_eq!(discovered["total"], 2);
    let period = store
        .records(
            id,
            ListQuery {
                kind: Some("requirement".into()),
                metric: Some(ProductivityMetric::Requirements),
                period: Some(true),
                days: Some(7),
                ..Default::default()
            },
        )
        .await
        .expect("valid fixture");
    assert_eq!(period["total"], 2);
    let ready = store
        .records(
            id,
            ListQuery {
                kind: Some("requirement".into()),
                status: Some("待发布".into()),
                ..Default::default()
            },
        )
        .await
        .expect("valid fixture");
    assert_eq!(ready["total"], 1);
    assert_eq!(ready["items"][0]["status"], "待发布");
    let late = store
        .records(
            id,
            ListQuery {
                kind: Some("requirement".into()),
                delayed_only: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("valid fixture");
    assert_eq!(late["total"], 1);
    assert_eq!(late["items"][0]["externalId"], "late");
    assert_eq!(store.dashboard(id, 7).await.expect("valid fixture")["totals"]["delayed"], 1);
    assert!(store.bug_statistics("missing-source", BugStatisticsQuery::default()).await.is_err());
    for table in ["ms_devops_history", "ms_devops_entity", "ms_devops_sync_run", "ms_devops_source"]
    {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE {}=$1",
            if table == "ms_devops_source" { "id" } else { "source_id" }
        ))
        .bind(id)
        .execute(&pool)
        .await
        .expect("valid fixture");
    }
}

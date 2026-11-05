#![cfg(feature = "http")]
use rag::{
    adapters::PgVectorStore,
    domain::{Audience, RagChunk, RagDocument},
    ports::VectorStore,
};
use serde_json::json;
#[tokio::test]
#[ignore = "requires isolated SHEPHERD_RAG_TEST_DATABASE_URL"]
async fn activity_uses_exact_author_dates_project_and_current_document_audience() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_RAG_TEST_DATABASE_URL").expect("isolated database"),
    )
    .await
    .expect("connect");
    migrate::run(&pool).await.expect("migrate");
    let store = PgVectorStore::new(pool.clone());
    for (id, project, owner, groups, date) in [
        ("own", "p", "owner", vec![], "2026-11-04"),
        ("private", "p", "other", vec![], "2026-11-04"),
        ("group", "p", "other", vec!["readers".into()], "2026-11-04"),
        ("foreign", "q", "owner", vec![], "2026-11-04"),
        ("old", "p", "owner", vec![], "2026-10-04"),
        ("part2", "p", "owner", vec![], "2026-11-04"),
    ] {
        let source = json!({"author":"崔少平","aliases":["cxl"],"date":date,"part":if id=="part2" {2} else {1}}).to_string();
        store
            .upsert_document(&RagDocument {
                id: id.into(),
                project_id: project.into(),
                source_type: "git_activity".into(),
                source_id: Some(source),
                title: id.into(),
                owner_id: Some(owner.into()),
                visibility_groups: groups,
                created_at: 0,
                updated_at: 0,
            })
            .await
            .expect("doc");
        store
            .replace_chunks(
                id,
                &[RagChunk {
                    id: format!("{id}-c"),
                    document_id: id.into(),
                    project_id: project.into(),
                    chunk_index: 0,
                    heading: "Git记录".into(),
                    content: "修改支付校验；1次提交；引用SHA".into(),
                    embedding: vec![],
                    created_at: 0,
                }],
            )
            .await
            .expect("chunk");
    }
    let owner = Audience { user_id: Some("owner".into()), ..Default::default() };
    let q = "cxl在2026-11-04干了什么";
    let own = store.activity_search("p", q, &owner).await.expect("query").expect("activity");
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].document_id, "own");
    assert!(own[0].content.contains("修改支付校验"));
    let at = chrono::DateTime::parse_from_rfc3339("2026-11-04T23:59:59+08:00")
        .expect("clock")
        .timestamp_millis();
    let today = store
        .activity_search_at("p", "今天git变化怎么样", &owner, at)
        .await
        .expect("today")
        .expect("activity");
    assert_eq!(today.len(), 1);
    assert_eq!(today[0].document_id, "own");
    assert!(today[0].content.contains("2026-11-04 至 2026-11-04"));
    let tomorrow = store
        .activity_search_at("p", "今天git变化怎么样", &owner, at + 2000)
        .await
        .expect("next day")
        .expect("activity");
    assert!(tomorrow.is_empty(), "never replace missing today's facts with older dates");
    assert!(store
        .activity_search_at("p", "张三今天git变化怎么样", &owner, at)
        .await
        .expect("unknown author")
        .expect("activity")
        .is_empty());
    let reader = Audience {
        user_id: Some("reader".into()),
        visible_group_ids: vec!["readers".into()],
        is_admin: false,
    };
    assert_eq!(
        store.activity_search("p", q, &reader).await.expect("query").expect("activity")[0]
            .document_id,
        "group"
    );
    assert!(store
        .activity_search("p", q, &Audience::default())
        .await
        .expect("query")
        .expect("activity")
        .is_empty());
    assert!(store
        .activity_search("p", "张三在2026-11-04干了什么", &owner)
        .await
        .expect("query")
        .expect("activity")
        .is_empty());
    assert!(store.activity_search("p", "支付接口怎么实现", &owner).await.expect("query").is_none());
    assert!(store.activity_search("p", "日报接口", &owner).await.expect("query").is_none());
    let admin = Audience { is_admin: true, ..Default::default() };
    assert_eq!(
        store.activity_search("p", q, &admin).await.expect("query").expect("activity").len(),
        3
    );
}

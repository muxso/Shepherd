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
    ] {
        let source = json!({"author":"崔少平","aliases":["cxl"],"date":date}).to_string();
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

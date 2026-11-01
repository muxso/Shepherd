#![cfg(feature = "pg")]
use case_management::{adapters::pg::PgCaseRepository, ports::CaseRepository};

#[tokio::test]
#[ignore = "requires isolated SHEPHERD_COVERAGE_TEST_DATABASE_URL"]
async fn summary_deduplicates_criteria_and_excludes_deleted_or_foreign_cases() {
    let pool = sqlx::PgPool::connect(
        &std::env::var("SHEPHERD_COVERAGE_TEST_DATABASE_URL").expect("isolated database"),
    )
    .await
    .expect("connect");
    migrate::run(&pool).await.expect("migrate");
    for (id, project, deleted) in
        [("req-a", "a", false), ("req-b", "b", false), ("req-deleted", "a", true)]
    {
        sqlx::query("INSERT INTO ms_requirement(id,project_id,title,deleted) VALUES($1,$2,$1,$3)")
            .bind(id)
            .bind(project)
            .bind(deleted)
            .execute(&pool)
            .await
            .expect("requirement");
    }
    for (id, project, deleted) in [
        ("case-a1", "a", false),
        ("case-a2", "a", false),
        ("case-b", "b", false),
        ("case-deleted", "a", true),
    ] {
        sqlx::query(
            "INSERT INTO ms_functional_case(id,project_id,name,deleted) VALUES($1,$2,$1,$3)",
        )
        .bind(id)
        .bind(project)
        .bind(deleted)
        .execute(&pool)
        .await
        .expect("case");
    }
    let repo = PgCaseRepository::new(pool.clone());
    for (req, index, case) in [
        ("req-a", 0, "case-a1"),
        ("req-a", 0, "case-a2"),
        ("req-a", 1, "case-a2"),
        ("req-a", 2, "case-b"),
        ("req-a", 3, "case-deleted"),
        ("req-b", 0, "case-b"),
        ("req-deleted", 0, "case-a1"),
    ] {
        // Legacy links can have an empty project reference; ownership comes from the actual resources.
        repo.link_requirement_case(req, index, case, "").await.expect("link");
    }
    let summary = repo.project_coverage("a").await.expect("summary");
    assert_eq!(summary.len(), 1);
    assert_eq!(summary["req-a"], vec![0, 1]);
    assert_eq!(repo.project_coverage("b").await.expect("b")["req-b"], vec![0]);
    assert!(repo.project_coverage("empty").await.expect("empty").is_empty());
    pool.close().await;
}

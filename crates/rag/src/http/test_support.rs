use super::*;
use crate::domain::Result;
use async_trait::async_trait;
use webauth::{testing::InMemorySessionStore, PermissionSet};
struct Fake;
#[async_trait]
impl Embedder for Fake {
    fn dim(&self) -> usize {
        1
    }
    async fn embed(&self, _: &str) -> Result<Vec<f32>> {
        Ok(vec![1.0])
    }
}
#[async_trait]
impl Chat for Fake {
    async fn complete(&self, _: &str, _: &str) -> Result<String> {
        Ok(serde_json::json!({"verdict":"APPROVE","summary":"Review with evidence","findings":[{"category":"test","severity":"high","title":"Validate boundary","detail":"Verify empty input","evidenceIds":["E1"],"confidence":"high"}]}).to_string())
    }
}
pub async fn fixture() -> (RagState, AuthUser, String, String) {
    let url = std::env::var("SHEPHERD_RAG_TEST_DATABASE_URL").expect("isolated URL");
    assert!(url.contains("127.0.0.1") && url.contains("test"));
    let pool = sqlx::PgPool::connect(&url).await.expect("connect");
    migrate::run(&pool).await.expect("migrate");
    let project:String=sqlx::query_scalar("INSERT INTO ms_project(organization_id,name) VALUES('test',gen_random_uuid()::text) RETURNING id").fetch_one(&pool).await.expect("project");
    let user = AuthUser {
        user_id: uuid::Uuid::new_v4().to_string(),
        permissions: PermissionSet::from_raw([
            "RAG:READ",
            "RAG:ADD",
            "PROJECT:READ",
            "PROJECT:UPDATE",
            "REQUIREMENT:READ",
            "REQUIREMENT:UPDATE",
            "FUNCTIONAL_CASE:READ",
            "FUNCTIONAL_CASE:ADD",
            "API_DEFINITION:READ",
        ])
        .expect("permissions"),
    };
    sqlx::query("INSERT INTO ms_project_member(project_id,user_id,role) VALUES($1,$2,'OWNER')")
        .bind(&project)
        .bind(&user.user_id)
        .execute(&pool)
        .await
        .expect("member");
    let requirement:String=sqlx::query_scalar("INSERT INTO ms_requirement(project_id,title,created_by) VALUES($1,'Order validation',$2) RETURNING id").bind(&project).bind(&user.user_id).fetch_one(&pool).await.expect("requirement");
    sqlx::query("INSERT INTO ms_requirement_version(requirement_id,version,description,acceptance_criteria) VALUES($1,1,'Validate order boundaries',ARRAY['Reject empty order'])").bind(&requirement).execute(&pool).await.expect("version");
    let state = RagState {
        store: Arc::new(crate::adapters::PgVectorStore::new(pool.clone())),
        embedder: Arc::new(Fake),
        chat: Arc::new(Fake),
        sessions: Arc::new(InMemorySessionStore::new()),
        config: crate::config::RagConfig::handle_from_env(),
        pool,
        changes: None,
    };
    (state, user, project, requirement)
}
pub async fn body(r: Response) -> serde_json::Value {
    let status = r.status();
    let bytes = axum::body::to_bytes(r.into_body(), 4_000_000).await.expect("body");
    assert!(status.is_success(), "{status}: {}", String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).expect("JSON")
}

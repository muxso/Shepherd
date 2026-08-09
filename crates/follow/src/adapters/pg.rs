use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::Follow;
use crate::ports::{FollowStore, RepoError};

#[derive(Clone)]
pub struct PgFollowStore {
    pool: PgPool,
}

impl PgFollowStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

#[async_trait]
impl FollowStore for PgFollowStore {
    async fn follow(&self, f: &Follow) -> Result<bool, RepoError> {
        let res = sqlx::query(
            "INSERT INTO ms_follow (project_id, entity_type, entity_id, user_id) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(&f.project_id)
        .bind(&f.entity_type)
        .bind(&f.entity_id)
        .bind(&f.user_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn unfollow(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: &str,
    ) -> Result<bool, RepoError> {
        let res = sqlx::query(
            "DELETE FROM ms_follow \
             WHERE project_id = $1 AND entity_type = $2 AND entity_id = $3 AND user_id = $4",
        )
        .bind(project_id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn followers(
        &self,
        project_id: &str,
        entity_type: &str,
        entity_id: &str,
    ) -> Result<Vec<String>, RepoError> {
        let rows = sqlx::query(
            "SELECT user_id FROM ms_follow \
             WHERE project_id = $1 AND entity_type = $2 AND entity_id = $3 \
             ORDER BY created_at, user_id",
        )
        .bind(project_id)
        .bind(entity_type)
        .bind(entity_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(|r| r.try_get("user_id").map_err(map_err)).collect()
    }

    async fn following_ids(
        &self,
        project_id: &str,
        user_id: &str,
        entity_type: Option<&str>,
    ) -> Result<Vec<String>, RepoError> {
        let rows = sqlx::query(
            "SELECT entity_id FROM ms_follow \
             WHERE project_id = $1 AND user_id = $2 AND ($3::text IS NULL OR entity_type = $3) \
             ORDER BY created_at, entity_id",
        )
        .bind(project_id)
        .bind(user_id)
        .bind(entity_type)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(|r| r.try_get("entity_id").map_err(map_err)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL pointing to a PostgreSQL instance"]
    async fn pg_follow_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_follow").execute(&pool).await.expect("truncate");

        let store = PgFollowStore::new(pool.clone());
        let f = Follow::new("p1", "bug", "b1", "alice").expect("valid");

        assert!(store.follow(&f).await.expect("follow"));
        assert!(!store.follow(&f).await.expect("follow"));
        assert_eq!(
            store.followers("p1", "bug", "b1").await.expect("ls"),
            vec!["alice".to_string()]
        );
        assert_eq!(
            store.following_ids("p1", "alice", Some("bug")).await.expect("mine"),
            vec!["b1".to_string()]
        );
        assert_eq!(
            store.following_ids("p1", "alice", None).await.expect("mine"),
            vec!["b1".to_string()]
        );

        assert!(store.unfollow("p1", "bug", "b1", "alice").await.expect("unfollow"));
        assert!(!store.unfollow("p1", "bug", "b1", "alice").await.expect("unfollow"));
        assert!(store.followers("p1", "bug", "b1").await.expect("ls").is_empty());
    }
}

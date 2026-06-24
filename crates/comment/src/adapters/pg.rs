//! PostgreSQL 实现的 `CommentRepository`。
//!
//! `created_at` 以 `created_at::text` 取 String,避免引入 chrono/time 依赖
//!(与 api-definition / api-scenario 等模块一致)。

use async_trait::async_trait;

use crate::domain::{Comment, NewComment};
use crate::ports::{CommentRepository, RepoError};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgCommentRepository {
    pool: PgPool,
}

impl PgCommentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

const COLS: &str =
    "id, target_type, target_id, content, author, created_at::text AS created_at, deleted";

fn row_to_comment(row: &sqlx::postgres::PgRow) -> Result<Comment, RepoError> {
    Ok(Comment {
        id: row.try_get("id").map_err(map_err)?,
        target_type: row.try_get("target_type").map_err(map_err)?,
        target_id: row.try_get("target_id").map_err(map_err)?,
        content: row.try_get("content").map_err(map_err)?,
        author: row.try_get("author").map_err(map_err)?,
        created_at: row.try_get("created_at").map_err(map_err)?,
        deleted: row.try_get("deleted").map_err(map_err)?,
    })
}

#[async_trait]
impl CommentRepository for PgCommentRepository {
    async fn insert(&self, new_comment: &NewComment) -> Result<Comment, RepoError> {
        let row = sqlx::query(&format!(
            "INSERT INTO ms_comment (target_type, target_id, content, author) \
             VALUES ($1, $2, $3, $4) RETURNING {COLS}",
        ))
        .bind(&new_comment.target_type)
        .bind(&new_comment.target_id)
        .bind(&new_comment.content)
        .bind(&new_comment.author)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_comment(&row)
    }

    async fn list(&self, target_type: &str, target_id: &str) -> Result<Vec<Comment>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_comment \
             WHERE target_type = $1 AND target_id = $2 AND deleted = false \
             ORDER BY created_at ASC, id ASC",
        ))
        .bind(target_type)
        .bind(target_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_comment).collect()
    }

    async fn get(&self, id: &str) -> Result<Option<Comment>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_comment WHERE id = $1 AND deleted = false",
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_comment).transpose()
    }

    async fn soft_delete(&self, id: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_comment SET deleted = true WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_comment_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_comment").execute(&pool).await.expect("truncate");

        let repo = PgCommentRepository::new(pool.clone());
        let nc = NewComment::new("BUG", "b1", "登录崩溃复现", "admin").expect("valid");
        let c = repo.insert(&nc).await.expect("insert");
        assert!(!c.created_at.is_empty());

        let list = repo.list("BUG", "b1").await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].content, "登录崩溃复现");

        repo.soft_delete(&c.id).await.expect("delete");
        assert!(repo.list("BUG", "b1").await.expect("list").is_empty());
        assert!(repo.get(&c.id).await.expect("get").is_none());
    }
}

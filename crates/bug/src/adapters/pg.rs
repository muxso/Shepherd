use async_trait::async_trait;
use crate::domain::{Bug, NewBug, StatusFlowGraph, StatusItem};
use crate::ports::{BugRepository, RepoError};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgBugRepository {
    pool: PgPool,
}

impl PgBugRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn row_to_bug(row: &sqlx::postgres::PgRow) -> Result<Bug, RepoError> {
    Ok(Bug {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        title: row.try_get("title").map_err(map_err)?,
        status: row.try_get("status").map_err(map_err)?,
        deleted: row.try_get("deleted").map_err(map_err)?,
        created_at: row.try_get("created_at").map_err(map_err)?,
        created_by: row.try_get("created_by").map_err(map_err)?,
    })
}

#[async_trait]
impl BugRepository for PgBugRepository {
    async fn status_flow(&self, project_id: &str) -> Result<StatusFlowGraph, RepoError> {
        let item_rows =
            sqlx::query("SELECT id, name, internal FROM ms_status_item WHERE project_id = $1")
                .bind(project_id)
                .fetch_all(&self.pool)
                .await
                .map_err(map_err)?;
        let mut items = Vec::with_capacity(item_rows.len());
        for r in &item_rows {
            let id: String = r.try_get("id").map_err(map_err)?;
            let name: String = r.try_get("name").map_err(map_err)?;
            let internal: bool = r.try_get("internal").map_err(map_err)?;
            items.push(StatusItem { id, name, internal });
        }

        // 未单独配置状态流的项目回落到领域默认种子流,保证缺陷功能开箱即用。
        if items.is_empty() {
            return Ok(StatusFlowGraph::default_bug_flow());
        }

        let edge_rows =
            sqlx::query("SELECT from_id, to_id FROM ms_status_flow WHERE project_id = $1")
                .bind(project_id)
                .fetch_all(&self.pool)
                .await
                .map_err(map_err)?;
        let mut edges = Vec::with_capacity(edge_rows.len());
        for r in &edge_rows {
            let from: String = r.try_get("from_id").map_err(map_err)?;
            let to: String = r.try_get("to_id").map_err(map_err)?;
            edges.push((from, to));
        }

        Ok(StatusFlowGraph::new(items, edges))
    }

    async fn insert(&self, new_bug: &NewBug, initial_status: &str) -> Result<Bug, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_bug (project_id, title, status, created_by) VALUES ($1, $2, $3, $4) \
             RETURNING id, project_id, title, status, deleted, created_at, created_by",
        )
        .bind(&new_bug.project_id)
        .bind(&new_bug.title)
        .bind(initial_status)
        .bind(&new_bug.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_bug(&row)
    }

    async fn list(&self, project_id: &str) -> Result<Vec<Bug>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, project_id, title, status, deleted, created_at, created_by FROM ms_bug \
             WHERE project_id = $1 AND deleted = false ORDER BY created_at DESC, id DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_bug).collect()
    }

    async fn get(&self, id: &str) -> Result<Option<Bug>, RepoError> {
        let row = sqlx::query(
            "SELECT id, project_id, title, status, deleted, created_at, created_by FROM ms_bug \
             WHERE id = $1 AND deleted = false",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_bug).transpose()
    }

    async fn set_status(&self, id: &str, status: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_bug SET status = $2 WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn add_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_bug_follower (bug_id, user_id) VALUES ($1, $2) \
             ON CONFLICT (bug_id, user_id) DO NOTHING",
        )
        .bind(bug_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn remove_follower(&self, bug_id: &str, user_id: &str) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM ms_bug_follower WHERE bug_id = $1 AND user_id = $2")
            .bind(bug_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn list_followers(&self, bug_id: &str) -> Result<Vec<String>, RepoError> {
        let rows = sqlx::query(
            "SELECT user_id FROM ms_bug_follower WHERE bug_id = $1 ORDER BY created_at, user_id",
        )
        .bind(bug_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(|r| r.try_get("user_id").map_err(map_err)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_status_flow_assembled_and_bug_status_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_status_item, ms_status_flow, ms_bug")
            .execute(&pool)
            .await
            .expect("truncate");

        sqlx::raw_sql(
            "INSERT INTO ms_status_item (id, project_id, name, internal) VALUES \
                ('NEW','p1','新建',true),('RESOLVED','p1','已解决',true),('CLOSED','p1','已关闭',true); \
             INSERT INTO ms_status_flow (project_id, from_id, to_id) VALUES \
                ('p1','NEW','RESOLVED'),('p1','RESOLVED','CLOSED');",
        )
        .execute(&pool)
        .await
        .expect("seed flow");

        let repo = PgBugRepository::new(pool.clone());

        let g = repo.status_flow("p1").await.expect("flow");
        assert!(g.can_transition("NEW", "RESOLVED"));
        assert!(!g.can_transition("NEW", "CLOSED"));
        assert_eq!(g.targets("RESOLVED"), vec!["CLOSED"]);

        let nb = NewBug::new("p1", "登录崩溃").expect("valid");
        let bug = repo.insert(&nb, "NEW").await.expect("insert");
        assert_eq!(repo.get(&bug.id).await.expect("get").expect("some").status, "NEW");

        repo.set_status(&bug.id, "RESOLVED").await.expect("set");
        assert_eq!(repo.get(&bug.id).await.expect("get").expect("some").status, "RESOLVED");

        let listed = repo.list("p1").await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, bug.id);
        assert_eq!(listed[0].title, "登录崩溃");
        assert!(repo.list("other").await.expect("list").is_empty());

        assert!(repo.get("ghost").await.expect("get").is_none());
    }
}

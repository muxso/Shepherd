use std::collections::BTreeMap;

use crate::domain::{Bug, BugRelation, NewBug, StatusFlowGraph, StatusItem};
use crate::ports::{BugRepository, RepoError};
use async_trait::async_trait;
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

/// Custom field map → JSONB object (values always strings).
fn fields_to_json(f: &BTreeMap<String, String>) -> serde_json::Value {
    serde_json::Value::Object(
        f.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect(),
    )
}

/// JSONB object → custom field map; non-string values (shouldn't occur) fall back to JSON text.
fn json_to_fields(v: &serde_json::Value) -> BTreeMap<String, String> {
    v.as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, val)| {
                    let s = match val {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), s)
                })
                .collect()
        })
        .unwrap_or_default()
}

const BUG_COLS: &str =
    "id, project_id, title, status, deleted, created_at, created_by, custom_fields";

fn row_to_bug(row: &sqlx::postgres::PgRow) -> Result<Bug, RepoError> {
    let custom: serde_json::Value = row.try_get("custom_fields").map_err(map_err)?;
    Ok(Bug {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        title: row.try_get("title").map_err(map_err)?,
        status: row.try_get("status").map_err(map_err)?,
        deleted: row.try_get("deleted").map_err(map_err)?,
        created_at: row.try_get("created_at").map_err(map_err)?,
        created_by: row.try_get("created_by").map_err(map_err)?,
        custom_fields: json_to_fields(&custom),
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

        // Projects without a configured status flow fall back to the domain default
        // seed flow so bugs work out of the box.
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
        let row = sqlx::query(&format!(
            "INSERT INTO ms_bug (project_id, title, status, created_by, custom_fields) \
             VALUES ($1, $2, $3, $4, $5) RETURNING {BUG_COLS}"
        ))
        .bind(&new_bug.project_id)
        .bind(&new_bug.title)
        .bind(initial_status)
        .bind(&new_bug.created_by)
        .bind(fields_to_json(&new_bug.custom_fields))
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_bug(&row)
    }

    async fn list(&self, project_id: &str) -> Result<Vec<Bug>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {BUG_COLS} FROM ms_bug \
             WHERE project_id = $1 AND deleted = false ORDER BY created_at DESC, id DESC"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_bug).collect()
    }

    async fn get(&self, id: &str) -> Result<Option<Bug>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {BUG_COLS} FROM ms_bug WHERE id = $1 AND deleted = false"
        ))
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

    async fn set_custom_fields(
        &self,
        id: &str,
        fields: &BTreeMap<String, String>,
    ) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_bug SET custom_fields = $2 WHERE id = $1")
            .bind(id)
            .bind(fields_to_json(fields))
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

    async fn add_relation(&self, rel: &BugRelation) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_bug_relation (bug_id, kind, target_id) VALUES ($1, $2, $3) \
             ON CONFLICT (bug_id, kind, target_id) DO NOTHING",
        )
        .bind(&rel.bug_id)
        .bind(rel.kind.as_str())
        .bind(&rel.target_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn remove_relation(&self, rel: &BugRelation) -> Result<(), RepoError> {
        sqlx::query(
            "DELETE FROM ms_bug_relation WHERE bug_id = $1 AND kind = $2 AND target_id = $3",
        )
        .bind(&rel.bug_id)
        .bind(rel.kind.as_str())
        .bind(&rel.target_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn list_relations(&self, bug_id: &str) -> Result<Vec<BugRelation>, RepoError> {
        let rows = sqlx::query(
            "SELECT bug_id, kind, target_id FROM ms_bug_relation WHERE bug_id = $1 \
             ORDER BY created_at, kind, target_id",
        )
        .bind(bug_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                let bug_id: String = r.try_get("bug_id").map_err(map_err)?;
                let kind: String = r.try_get("kind").map_err(map_err)?;
                let target_id: String = r.try_get("target_id").map_err(map_err)?;
                BugRelation::new(&bug_id, &kind, &target_id)
                    .map_err(|e| RepoError::Backend(e.to_string()))
            })
            .collect()
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
        // CASCADE: ms_bug_follower / ms_bug_relation hang off ms_bug via foreign keys.
        sqlx::raw_sql("TRUNCATE ms_status_item, ms_status_flow, ms_bug CASCADE")
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

        let cf = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let nb = NewBug::new("p1", "登录崩溃")
            .expect("valid")
            .with_custom_fields(cf(&[("severity", "P0"), ("多选", "a,b")]));
        let bug = repo.insert(&nb, "NEW").await.expect("insert");
        let got = repo.get(&bug.id).await.expect("get").expect("some");
        assert_eq!(got.status, "NEW");
        // Custom fields read back as written.
        assert_eq!(got.custom_fields, cf(&[("severity", "P0"), ("多选", "a,b")]));

        repo.set_status(&bug.id, "RESOLVED").await.expect("set");
        assert_eq!(repo.get(&bug.id).await.expect("get").expect("some").status, "RESOLVED");

        // Full replacement; empty map clears all.
        repo.set_custom_fields(&bug.id, &cf(&[("env", "prod")])).await.expect("set fields");
        assert_eq!(
            repo.get(&bug.id).await.expect("get").expect("some").custom_fields,
            cf(&[("env", "prod")])
        );
        repo.set_custom_fields(&bug.id, &BTreeMap::new()).await.expect("clear fields");
        assert!(repo.get(&bug.id).await.expect("get").expect("some").custom_fields.is_empty());

        let listed = repo.list("p1").await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, bug.id);
        assert_eq!(listed[0].title, "登录崩溃");
        assert!(repo.list("other").await.expect("list").is_empty());

        assert!(repo.get("ghost").await.expect("get").is_none());
    }
}

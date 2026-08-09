use std::collections::BTreeMap;

use crate::domain::{Bug, BugRelation, NewBug, RelationKind, StatusFlowGraph, StatusItem};
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

const BUG_COLS: &str = "id, project_id, title, status, deleted, created_at, created_by, \
     severity, handler, description, updated_by, updated_at::text AS updated_at, custom_fields";

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
        severity: row.try_get("severity").map_err(map_err)?,
        handler: row.try_get("handler").map_err(map_err)?,
        description: row.try_get("description").map_err(map_err)?,
        updated_by: row.try_get("updated_by").map_err(map_err)?,
        updated_at: row.try_get("updated_at").map_err(map_err)?,
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
        // updated_by/updated_at seed from the creator so the audit pair is never empty.
        let row = sqlx::query(&format!(
            "INSERT INTO ms_bug (project_id, title, status, created_by, custom_fields, \
             severity, handler, description, updated_by, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $4, now()) RETURNING {BUG_COLS}"
        ))
        .bind(&new_bug.project_id)
        .bind(&new_bug.title)
        .bind(initial_status)
        .bind(&new_bug.created_by)
        .bind(fields_to_json(&new_bug.custom_fields))
        .bind(&new_bug.severity)
        .bind(&new_bug.handler)
        .bind(&new_bug.description)
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

    async fn set_status(
        &self,
        id: &str,
        status: &str,
        operator: Option<&str>,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE ms_bug SET status = $2, updated_by = $3, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(operator)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn update_meta(
        &self,
        id: &str,
        title: &str,
        severity: Option<&str>,
        handler: Option<&str>,
        description: Option<&str>,
        operator: Option<&str>,
    ) -> Result<Option<Bug>, RepoError> {
        let row = sqlx::query(&format!(
            "UPDATE ms_bug SET title = $2, severity = $3, handler = $4, \
             description = COALESCE($6, description), \
             updated_by = $5, updated_at = now() \
             WHERE id = $1 AND deleted = false RETURNING {BUG_COLS}"
        ))
        .bind(id)
        .bind(title)
        .bind(severity)
        .bind(handler)
        .bind(operator)
        .bind(description)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_bug).transpose()
    }

    async fn set_custom_fields(
        &self,
        id: &str,
        fields: &BTreeMap<String, String>,
        operator: Option<&str>,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE ms_bug SET custom_fields = $2, updated_by = $3, updated_at = now() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(fields_to_json(fields))
        .bind(operator)
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

    async fn list_bugs_by_relation(
        &self,
        kind: RelationKind,
        target_id: &str,
    ) -> Result<Vec<Bug>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {BUG_COLS} FROM ms_bug WHERE deleted = false AND id IN \
             (SELECT bug_id FROM ms_bug_relation WHERE kind = $1 AND target_id = $2) \
             ORDER BY created_at DESC, id DESC"
        ))
        .bind(kind.as_str())
        .bind(target_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_bug).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL pointing to a PostgreSQL instance"]
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
                ('NEW','p1','New',true),('RESOLVED','p1','Resolved',true),('CLOSED','p1','Closed',true); \
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
        let nb = NewBug::new("p1", "login crash")
            .expect("valid")
            .with_created_by(Some("alice"))
            .with_severity(Some("P1".into()))
            .with_handler(Some("bob".into()))
            .with_custom_fields(cf(&[("severity", "P0"), ("multi-select", "a,b")]));
        let bug = repo.insert(&nb, "NEW").await.expect("insert");
        let got = repo.get(&bug.id).await.expect("get").expect("some");
        assert_eq!(got.status, "NEW");
        // First-class fields land; the audit pair seeds from the creator.
        assert_eq!(got.severity.as_deref(), Some("P1"));
        assert_eq!(got.handler.as_deref(), Some("bob"));
        assert_eq!(got.updated_by.as_deref(), Some("alice"));
        assert!(got.updated_at.is_some());
        // Custom fields read back as written.
        assert_eq!(got.custom_fields, cf(&[("severity", "P0"), ("multi-select", "a,b")]));

        repo.set_status(&bug.id, "RESOLVED", Some("carol")).await.expect("set");
        let after = repo.get(&bug.id).await.expect("get").expect("some");
        assert_eq!(after.status, "RESOLVED");
        assert_eq!(after.updated_by.as_deref(), Some("carol"));

        // Meta update replaces severity/handler and stamps the operator.
        let updated = repo
            .update_meta(&bug.id, "login crash (modified)", Some("P0"), None, None, Some("dave"))
            .await
            .expect("meta")
            .expect("some");
        assert_eq!(updated.title, "login crash (modified)");
        assert_eq!(updated.severity.as_deref(), Some("P0"));
        assert_eq!(updated.handler, None);
        assert_eq!(updated.updated_by.as_deref(), Some("dave"));
        assert!(repo
            .update_meta("ghost", "x", None, None, None, None)
            .await
            .expect("meta")
            .is_none());

        // Full replacement; empty map clears all.
        repo.set_custom_fields(&bug.id, &cf(&[("env", "prod")]), Some("erin"))
            .await
            .expect("set fields");
        let after = repo.get(&bug.id).await.expect("get").expect("some");
        assert_eq!(after.custom_fields, cf(&[("env", "prod")]));
        assert_eq!(after.updated_by.as_deref(), Some("erin"));
        repo.set_custom_fields(&bug.id, &BTreeMap::new(), None).await.expect("clear fields");
        assert!(repo.get(&bug.id).await.expect("get").expect("some").custom_fields.is_empty());

        // Relation reverse lookup: link to a plan, query by plan, then unlink.
        let rel = BugRelation::new(&bug.id, "PLAN", "plan-1").expect("rel");
        repo.add_relation(&rel).await.expect("add rel");
        let linked = repo.list_bugs_by_relation(RelationKind::Plan, "plan-1").await.expect("rev");
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id, bug.id);
        assert!(repo
            .list_bugs_by_relation(RelationKind::Plan, "ghost")
            .await
            .expect("rev")
            .is_empty());
        repo.remove_relation(&rel).await.expect("rm rel");
        assert!(repo
            .list_bugs_by_relation(RelationKind::Plan, "plan-1")
            .await
            .expect("rev")
            .is_empty());

        let listed = repo.list("p1").await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, bug.id);
        assert_eq!(listed[0].title, "login crash (modified)");
        assert!(repo.list("other").await.expect("list").is_empty());

        assert!(repo.get("ghost").await.expect("get").is_none());
    }
}

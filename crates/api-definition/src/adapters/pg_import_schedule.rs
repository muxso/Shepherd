use async_trait::async_trait;

use crate::domain::{ImportSchedule, NewImportSchedule};
use crate::ports::{ImportScheduleStore, RepoError};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgImportScheduleStore {
    pool: PgPool,
}

impl PgImportScheduleStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

const COLS: &str = "id, project_id, name, format, source_url, auth_token, basic_auth, module_id, \
     group_by_tag, overwrite, sync_module, cron, enabled, \
     last_run_at::text AS last_run_at, last_result, last_run_by, created_by, created_at::text AS created_at";

fn row_to_schedule(row: &sqlx::postgres::PgRow) -> Result<ImportSchedule, RepoError> {
    Ok(ImportSchedule {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        name: row.try_get::<String, _>("name").unwrap_or_default(),
        format: row.try_get::<String, _>("format").unwrap_or_else(|_| "openapi".to_string()),
        source_url: row.try_get("source_url").map_err(map_err)?,
        auth_token: row.try_get::<String, _>("auth_token").unwrap_or_default(),
        basic_auth: row.try_get::<bool, _>("basic_auth").unwrap_or(false),
        module_id: row.try_get::<Option<String>, _>("module_id").map_err(map_err)?,
        group_by_tag: row.try_get::<bool, _>("group_by_tag").unwrap_or(true),
        overwrite: row.try_get::<bool, _>("overwrite").unwrap_or(true),
        sync_module: row.try_get::<bool, _>("sync_module").unwrap_or(false),
        cron: row.try_get("cron").map_err(map_err)?,
        enabled: row.try_get::<bool, _>("enabled").unwrap_or(true),
        last_run_at: row.try_get::<Option<String>, _>("last_run_at").unwrap_or(None),
        last_result: row.try_get::<String, _>("last_result").unwrap_or_default(),
        last_run_by: row.try_get::<String, _>("last_run_by").unwrap_or_default(),
        created_by: row.try_get::<String, _>("created_by").unwrap_or_default(),
        created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
    })
}

#[async_trait]
impl ImportScheduleStore for PgImportScheduleStore {
    async fn insert(&self, s: &NewImportSchedule) -> Result<ImportSchedule, RepoError> {
        let sql = format!(
            "INSERT INTO ms_api_import_schedule \
             (project_id, name, format, source_url, auth_token, basic_auth, module_id, \
              group_by_tag, overwrite, sync_module, cron, enabled, created_by) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING {COLS}"
        );
        let row = sqlx::query(&sql)
            .bind(&s.project_id)
            .bind(&s.name)
            .bind(&s.format)
            .bind(&s.source_url)
            .bind(&s.auth_token)
            .bind(s.basic_auth)
            .bind(&s.module_id)
            .bind(s.group_by_tag)
            .bind(s.overwrite)
            .bind(s.sync_module)
            .bind(&s.cron)
            .bind(s.enabled)
            .bind(&s.created_by)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)?;
        row_to_schedule(&row)
    }

    async fn list_by_project(&self, project_id: &str) -> Result<Vec<ImportSchedule>, RepoError> {
        let sql = format!(
            "SELECT {COLS} FROM ms_api_import_schedule \
             WHERE project_id = $1 AND deleted = false ORDER BY created_at DESC"
        );
        let rows =
            sqlx::query(&sql).bind(project_id).fetch_all(&self.pool).await.map_err(map_err)?;
        rows.iter().map(row_to_schedule).collect()
    }

    async fn list_enabled(&self) -> Result<Vec<ImportSchedule>, RepoError> {
        let sql = format!(
            "SELECT {COLS} FROM ms_api_import_schedule \
             WHERE enabled = true AND deleted = false ORDER BY created_at DESC"
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await.map_err(map_err)?;
        rows.iter().map(row_to_schedule).collect()
    }

    async fn get(&self, id: &str) -> Result<Option<ImportSchedule>, RepoError> {
        let sql =
            format!("SELECT {COLS} FROM ms_api_import_schedule WHERE id = $1 AND deleted = false");
        let row = sqlx::query(&sql).bind(id).fetch_optional(&self.pool).await.map_err(map_err)?;
        row.as_ref().map(row_to_schedule).transpose()
    }

    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_api_import_schedule SET enabled = $2 WHERE id = $1")
            .bind(id)
            .bind(enabled)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_api_import_schedule SET deleted = true WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn record_run(&self, id: &str, result: &str, operator: &str) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE ms_api_import_schedule \
             SET last_run_at = now(), last_result = $2, last_run_by = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(result)
        .bind(operator)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }
}

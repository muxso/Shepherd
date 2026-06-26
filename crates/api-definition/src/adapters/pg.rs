use async_trait::async_trait;

use crate::domain::{
    ApiCase, ApiDefinition, ApiDefinitionChange, ApiModule, ApiMock, ApiProtocol, ApiStatus,
    ApiView, NewApiCase, NewApiDefinition, NewApiModule, NewApiMock, NewApiView,
};
use crate::ports::{ApiDefinitionRepository, ProjectMockRow, RepoError};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgApiDefinitionRepository {
    pool: PgPool,
}

impl PgApiDefinitionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn row_to_definition(row: &sqlx::postgres::PgRow) -> Result<ApiDefinition, RepoError> {
    let protocol: String = row.try_get("protocol").map_err(map_err)?;
    let status: String = row.try_get("status").map_err(map_err)?;
    Ok(ApiDefinition {
        id: row.try_get("id").map_err(map_err)?,
        num: row.try_get::<i64, _>("num").unwrap_or(0),
        project_id: row.try_get("project_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        protocol: ApiProtocol::parse(&protocol).unwrap_or_default(),
        method: row.try_get("method").map_err(map_err)?,
        path: row.try_get("path").map_err(map_err)?,
        status: ApiStatus::parse(&status).unwrap_or_default(),
        module_id: row.try_get::<Option<String>, _>("module_id").map_err(map_err)?,
        spec: row.try_get::<String, _>("spec").unwrap_or_else(|_| "{}".to_string()),
        created_by: row.try_get::<String, _>("created_by").unwrap_or_default(),
        created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
        updated_at: row.try_get::<String, _>("updated_at").unwrap_or_default(),
    })
}

fn row_to_module(row: &sqlx::postgres::PgRow) -> Result<ApiModule, RepoError> {
    Ok(ApiModule {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        parent_id: row.try_get::<Option<String>, _>("parent_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
    })
}

fn row_to_case(row: &sqlx::postgres::PgRow) -> Result<ApiCase, RepoError> {
    Ok(ApiCase {
        id: row.try_get("id").map_err(map_err)?,
        api_definition_id: row.try_get("api_definition_id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        method: row.try_get("method").map_err(map_err)?,
        url: row.try_get("url").map_err(map_err)?,
        body: row.try_get("body").map_err(map_err)?,
        assertions: row.try_get("assertions").map_err(map_err)?,
        processors: row.try_get("processors").map_err(map_err)?,
        priority: row.try_get::<String, _>("priority").unwrap_or_else(|_| "P0".to_string()),
        status: row.try_get::<String, _>("status").unwrap_or_else(|_| "进行中".to_string()),
        tags: row.try_get("tags").unwrap_or_else(|_| serde_json::json!([])),
        headers: row.try_get("headers").unwrap_or_else(|_| serde_json::json!([])),
        query_params: row.try_get("query_params").unwrap_or_else(|_| serde_json::json!([])),
        rest_params: row.try_get("rest_params").unwrap_or_else(|_| serde_json::json!([])),
        auth: row.try_get("auth").unwrap_or_else(|_| serde_json::json!({})),
    })
}

fn row_to_view(row: &sqlx::postgres::PgRow) -> Result<ApiView, RepoError> {
    Ok(ApiView {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        user_id: row.try_get("user_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        config: row.try_get("config").unwrap_or_else(|_| serde_json::json!({})),
        shared: row.try_get("shared").unwrap_or(true),
        created_at: row.try_get::<String, _>("created_at").unwrap_or_default(),
    })
}

fn row_to_mock(row: &sqlx::postgres::PgRow) -> Result<ApiMock, RepoError> {
    Ok(ApiMock {
        id: row.try_get("id").map_err(map_err)?,
        api_definition_id: row.try_get("api_definition_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        match_rule: row.try_get("match_rule").map_err(map_err)?,
        response_status: row.try_get("response_status").map_err(map_err)?,
        response_body: row.try_get("response_body").map_err(map_err)?,
        enabled: row.try_get("enabled").map_err(map_err)?,
        tags: row.try_get("tags").unwrap_or_else(|_| serde_json::json!([])),
        response_headers: row.try_get("response_headers").unwrap_or_else(|_| serde_json::json!([])),
        response_delay_ms: row.try_get::<i32, _>("response_delay_ms").unwrap_or(0),
        follow_definition: row.try_get::<bool, _>("follow_definition").unwrap_or(false),
        created_by: row.try_get::<String, _>("created_by").unwrap_or_default(),
    })
}

#[async_trait]
impl ApiDefinitionRepository for PgApiDefinitionRepository {
    async fn insert_definition(
        &self,
        d: &NewApiDefinition,
    ) -> Result<ApiDefinition, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_definition (project_id, name, protocol, method, path, status, spec, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             RETURNING id, num, project_id, name, protocol, method, path, status, module_id, spec, \
                       created_by, created_at::text AS created_at, updated_at::text AS updated_at",
        )
        .bind(&d.project_id)
        .bind(&d.name)
        .bind(d.protocol.as_str())
        .bind(&d.method)
        .bind(&d.path)
        .bind(d.status.as_str())
        .bind(&d.spec)
        .bind(&d.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_definition(&row)
    }

    async fn get_definition(&self, id: &str) -> Result<Option<ApiDefinition>, RepoError> {
        let row = sqlx::query(
            "SELECT id, num, project_id, name, protocol, method, path, status, module_id, spec, \
                    created_by, created_at::text AS created_at, updated_at::text AS updated_at \
             FROM ms_api_definition WHERE id = $1 AND deleted = false",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_definition).transpose()
    }

    async fn update_definition(
        &self,
        id: &str,
        name: &str,
        protocol: &str,
        method: &str,
        path: &str,
    ) -> Result<Option<ApiDefinition>, RepoError> {
        let row = sqlx::query(
            "UPDATE ms_api_definition \
             SET name = $2, protocol = $3, method = $4, path = $5, updated_at = now() \
             WHERE id = $1 AND deleted = false \
             RETURNING id, num, project_id, name, protocol, method, path, status, module_id, spec, \
                       created_by, created_at::text AS created_at, updated_at::text AS updated_at",
        )
        .bind(id)
        .bind(name)
        .bind(protocol)
        .bind(method)
        .bind(path)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_definition).transpose()
    }

    async fn update_definition_spec(&self, id: &str, spec: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_api_definition SET spec = $2, updated_at = now() WHERE id = $1 AND deleted = false")
            .bind(id)
            .bind(spec)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn update_definition_status(&self, id: &str, status: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_api_definition SET status = $2, updated_at = now() WHERE id = $1 AND deleted = false")
            .bind(id)
            .bind(status)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn delete_definition(&self, id: &str) -> Result<(), RepoError> {
        // 用例为硬删:ms_api_case 无 deleted 列,不能软删。
        sqlx::query("UPDATE ms_api_definition SET deleted = true, updated_at = now() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        sqlx::query("UPDATE ms_api_mock SET deleted = true WHERE api_definition_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        sqlx::query("DELETE FROM ms_api_case WHERE api_definition_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn record_definition_change(
        &self,
        definition_id: &str,
        action: &str,
        detail: &str,
        actor: &str,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_api_definition_change (definition_id, action, detail, actor) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(definition_id)
        .bind(action)
        .bind(detail)
        .bind(actor)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn list_definition_changes(
        &self,
        definition_id: &str,
    ) -> Result<Vec<ApiDefinitionChange>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, definition_id, action, detail, actor, created_at::text AS created_at \
             FROM ms_api_definition_change WHERE definition_id = $1 \
             ORDER BY created_at DESC",
        )
        .bind(definition_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|row| {
                Ok(ApiDefinitionChange {
                    id: row.try_get("id").map_err(map_err)?,
                    definition_id: row.try_get("definition_id").map_err(map_err)?,
                    action: row.try_get("action").map_err(map_err)?,
                    detail: row.try_get("detail").map_err(map_err)?,
                    actor: row.try_get("actor").map_err(map_err)?,
                    created_at: row.try_get::<String, _>("created_at").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn list_definitions(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiDefinition>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, num, project_id, name, protocol, method, path, status, module_id, spec, \
                    created_by, created_at::text AS created_at, updated_at::text AS updated_at \
             FROM ms_api_definition WHERE project_id = $1 AND deleted = false \
             ORDER BY created_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_definition).collect()
    }

    async fn insert_case(&self, c: &NewApiCase) -> Result<ApiCase, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_case \
                (api_definition_id, project_id, name, method, url, body, assertions, processors, \
                 priority, case_status, tags, headers, query_params, rest_params, auth) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
             RETURNING id, api_definition_id, project_id, name, method, url, body, assertions, processors, \
                       priority, case_status AS status, tags, headers, query_params, rest_params, auth",
        )
        .bind(&c.api_definition_id)
        .bind(&c.project_id)
        .bind(&c.name)
        .bind(&c.method)
        .bind(&c.url)
        .bind(&c.body)
        .bind(&c.assertions)
        .bind(&c.processors)
        .bind(&c.priority)
        .bind(&c.status)
        .bind(&c.tags)
        .bind(&c.headers)
        .bind(&c.query_params)
        .bind(&c.rest_params)
        .bind(&c.auth)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_case(&row)
    }

    async fn update_case(&self, id: &str, c: &NewApiCase) -> Result<bool, RepoError> {
        let res = sqlx::query(
            "UPDATE ms_api_case SET name=$2, method=$3, url=$4, body=$5, assertions=$6, processors=$7, \
                    priority=$8, case_status=$9, tags=$10, headers=$11, query_params=$12, rest_params=$13, auth=$14 \
             WHERE id=$1",
        )
        .bind(id)
        .bind(&c.name)
        .bind(&c.method)
        .bind(&c.url)
        .bind(&c.body)
        .bind(&c.assertions)
        .bind(&c.processors)
        .bind(&c.priority)
        .bind(&c.status)
        .bind(&c.tags)
        .bind(&c.headers)
        .bind(&c.query_params)
        .bind(&c.rest_params)
        .bind(&c.auth)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn delete_case(&self, id: &str) -> Result<bool, RepoError> {
        let res = sqlx::query("DELETE FROM ms_api_case WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn update_mock(&self, mock_id: &str, m: &NewApiMock) -> Result<bool, RepoError> {
        let res = sqlx::query(
            "UPDATE ms_api_mock SET name=$2, match_rule=$3, response_status=$4, response_body=$5, \
                    enabled=$6, tags=$7, response_headers=$8, response_delay_ms=$9, \
                    follow_definition=$10, updated_at=now() \
             WHERE id=$1 AND deleted = false",
        )
        .bind(mock_id)
        .bind(&m.name)
        .bind(&m.match_rule)
        .bind(m.response_status)
        .bind(&m.response_body)
        .bind(m.enabled)
        .bind(&m.tags)
        .bind(&m.response_headers)
        .bind(m.response_delay_ms)
        .bind(m.follow_definition)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn delete_mock(&self, mock_id: &str) -> Result<bool, RepoError> {
        let res = sqlx::query("UPDATE ms_api_mock SET deleted = true WHERE id = $1 AND deleted = false")
            .bind(mock_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn list_cases(&self, api_definition_id: &str) -> Result<Vec<ApiCase>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, api_definition_id, project_id, name, method, url, body, assertions, processors, \
                    priority, case_status AS status, tags, headers, query_params, rest_params, auth \
             FROM ms_api_case WHERE api_definition_id = $1",
        )
        .bind(api_definition_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_case).collect()
    }

    async fn count_cases_by_project(&self, project_id: &str) -> Result<u64, RepoError> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM ms_api_case WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)?;
        let n: i64 = row.try_get("n").map_err(map_err)?;
        Ok(n as u64)
    }

    async fn list_cases_by_project(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<ApiCase>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, api_definition_id, project_id, name, method, url, body, assertions, processors, \
                    priority, case_status AS status, tags, headers, query_params, rest_params, auth \
             FROM ms_api_case WHERE project_id = $1 ORDER BY id LIMIT $2 OFFSET $3",
        )
        .bind(project_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_case).collect()
    }

    async fn insert_mock(&self, m: &NewApiMock) -> Result<ApiMock, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_mock \
                (api_definition_id, name, match_rule, response_status, response_body, enabled, \
                 tags, response_headers, response_delay_ms, follow_definition, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             RETURNING id, api_definition_id, name, match_rule, response_status, response_body, enabled, \
                       tags, response_headers, response_delay_ms, follow_definition, created_by",
        )
        .bind(&m.api_definition_id)
        .bind(&m.name)
        .bind(&m.match_rule)
        .bind(m.response_status)
        .bind(&m.response_body)
        .bind(m.enabled)
        .bind(&m.tags)
        .bind(&m.response_headers)
        .bind(m.response_delay_ms)
        .bind(m.follow_definition)
        .bind(&m.created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_mock(&row)
    }

    async fn list_mocks(&self, api_definition_id: &str) -> Result<Vec<ApiMock>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, api_definition_id, name, match_rule, response_status, response_body, enabled, \
                    tags, response_headers, response_delay_ms, follow_definition, created_by \
             FROM ms_api_mock WHERE api_definition_id = $1 AND deleted = false",
        )
        .bind(api_definition_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_mock).collect()
    }

    async fn list_mocks_by_project(&self, project_id: &str) -> Result<Vec<ProjectMockRow>, RepoError> {
        let rows = sqlx::query(
            "SELECT m.id, m.api_definition_id, m.name, m.match_rule, m.response_status, m.response_body, \
                    m.enabled, m.tags, m.response_headers, m.response_delay_ms, m.follow_definition, \
                    m.created_by, m.created_by AS m_operator, m.updated_at::text AS m_updated_at, \
                    d.method AS d_method, d.path AS d_path, d.protocol AS d_protocol, d.name AS d_name \
             FROM ms_api_mock m JOIN ms_api_definition d ON d.id = m.api_definition_id \
             WHERE d.project_id = $1 AND m.deleted = false AND d.deleted = false \
             ORDER BY m.updated_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                Ok(ProjectMockRow {
                    mock: row_to_mock(r)?,
                    method: r.try_get("d_method").map_err(map_err)?,
                    path: r.try_get("d_path").map_err(map_err)?,
                    protocol: r.try_get("d_protocol").map_err(map_err)?,
                    definition_name: r.try_get("d_name").map_err(map_err)?,
                    operator: r.try_get("m_operator").unwrap_or_default(),
                    updated_at: r.try_get("m_updated_at").unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn insert_module(&self, m: &NewApiModule) -> Result<ApiModule, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_module (project_id, parent_id, name) VALUES ($1, $2, $3) \
             RETURNING id, project_id, parent_id, name",
        )
        .bind(&m.project_id)
        .bind(&m.parent_id)
        .bind(&m.name)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_module(&row)
    }

    async fn list_modules(&self, project_id: &str) -> Result<Vec<ApiModule>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, project_id, parent_id, name FROM ms_api_module \
             WHERE project_id = $1 AND deleted = false ORDER BY name",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_module).collect()
    }

    async fn rename_module(&self, id: &str, name: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_api_module SET name = $2 WHERE id = $1")
            .bind(id)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn delete_module(&self, id: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_api_module SET deleted = true WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        sqlx::query("UPDATE ms_api_definition SET module_id = NULL WHERE module_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn set_definition_module(
        &self,
        definition_id: &str,
        module_id: Option<&str>,
    ) -> Result<(), RepoError> {
        sqlx::query("UPDATE ms_api_definition SET module_id = $2, updated_at = now() WHERE id = $1")
            .bind(definition_id)
            .bind(module_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn insert_view(&self, v: &NewApiView) -> Result<ApiView, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_view (project_id, user_id, name, config, shared) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, project_id, user_id, name, config, shared, created_at::text AS created_at",
        )
        .bind(&v.project_id)
        .bind(&v.user_id)
        .bind(&v.name)
        .bind(&v.config)
        .bind(v.shared)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(row_to_view(&row)?)
    }

    async fn list_views(&self, project_id: &str, user_id: &str) -> Result<Vec<ApiView>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, project_id, user_id, name, config, shared, created_at::text AS created_at \
             FROM ms_api_view WHERE project_id = $1 AND (shared OR user_id = $2) \
             ORDER BY created_at DESC",
        )
        .bind(project_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_view).collect()
    }

    async fn delete_view(&self, id: &str, user_id: &str) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM ms_api_view WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn link_task_case(
        &self,
        decomposition_id: &str,
        task_id: &str,
        case_id: &str,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_task_case (decomposition_id, task_id, case_id) VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
        )
        .bind(decomposition_id)
        .bind(task_id)
        .bind(case_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn unlink_task_case(
        &self,
        decomposition_id: &str,
        task_id: &str,
        case_id: &str,
    ) -> Result<(), RepoError> {
        sqlx::query(
            "DELETE FROM ms_task_case WHERE decomposition_id = $1 AND task_id = $2 AND case_id = $3",
        )
        .bind(decomposition_id)
        .bind(task_id)
        .bind(case_id)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn list_cases_for_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<ApiCase>, RepoError> {
        let rows = sqlx::query(
            "SELECT c.id, c.api_definition_id, c.project_id, c.name, c.method, c.url, c.body, \
                    c.assertions, c.processors, c.priority, c.case_status AS status, c.tags, c.headers, \
                    c.query_params, c.rest_params, c.auth \
             FROM ms_task_case tc JOIN ms_api_case c ON c.id = tc.case_id \
             WHERE tc.decomposition_id = $1 AND tc.task_id = $2",
        )
        .bind(decomposition_id)
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_case).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ApiProtocol;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_definition_case_mock_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_api_definition, ms_api_case, ms_api_mock")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgApiDefinitionRepository::new(pool.clone());

        let nd = NewApiDefinition::new("p1", "登录", ApiProtocol::Http, "POST", "/login")
            .expect("valid");
        let def = repo.insert_definition(&nd).await.expect("insert def");
        assert_eq!(def.method, "POST");
        assert_eq!(repo.get_definition(&def.id).await.expect("get").expect("some").name, "登录");
        assert_eq!(repo.list_definitions("p1").await.expect("list").len(), 1);
        assert!(repo.get_definition("ghost").await.expect("get").is_none());

        let nc = NewApiCase::new(
            &def.id,
            "p1",
            "用例",
            "POST",
            "/login",
            Some("{}".into()),
            serde_json::json!([{"type": "status", "value": 200}]),
        )
        .expect("valid");
        let case = repo.insert_case(&nc).await.expect("insert case");
        assert_eq!(case.method, "POST");
        let cases = repo.list_cases(&def.id).await.expect("list cases");
        assert_eq!(cases.len(), 1);
        assert!(cases[0].assertions.is_array());

        let standalone = NewApiCase::new(
            "",
            "p1",
            "独立用例",
            "GET",
            "/ping",
            None,
            serde_json::json!([]),
        )
        .expect("valid");
        repo.insert_case(&standalone).await.expect("insert standalone");
        assert_eq!(repo.count_cases_by_project("p1").await.expect("count"), 2);
        let page = repo.list_cases_by_project("p1", 0, 1).await.expect("page");
        assert_eq!(page.len(), 1);
        let page2 = repo.list_cases_by_project("p1", 1, 10).await.expect("page");
        assert_eq!(page2.len(), 1);

        let nm = NewApiMock::new(
            &def.id,
            "挡板",
            serde_json::json!({"path": "/login"}),
            200,
            Some("ok".into()),
            true,
        )
        .expect("valid");
        let mock = repo.insert_mock(&nm).await.expect("insert mock");
        assert_eq!(mock.response_status, 200);
        let mocks = repo.list_mocks(&def.id).await.expect("list mocks");
        assert_eq!(mocks.len(), 1);
        assert_eq!(mocks[0].match_rule, serde_json::json!({"path": "/login"}));

        let nm2 = NewApiMock::new(
            &def.id,
            "改名挡板",
            serde_json::json!({"path": "/logout"}),
            404,
            Some("nope".into()),
            false,
        )
        .expect("valid");
        assert!(repo.update_mock(&mock.id, &nm2).await.expect("update mock"));
        let mocks = repo.list_mocks(&def.id).await.expect("list mocks");
        assert_eq!(mocks.len(), 1);
        assert_eq!(mocks[0].name, "改名挡板");
        assert_eq!(mocks[0].response_status, 404);
        assert!(!mocks[0].enabled);
        assert_eq!(mocks[0].match_rule, serde_json::json!({"path": "/logout"}));
        assert!(!repo.update_mock("ghost", &nm2).await.expect("update missing"));
    }
}

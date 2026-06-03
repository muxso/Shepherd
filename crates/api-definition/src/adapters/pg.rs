//! PostgreSQL 实现的 `ApiDefinitionRepository`。
//!
//! 三张表对应三聚合:ms_api_definition / ms_api_case / ms_api_mock。
//! 协议与状态在表里以文本存储(HTTP/DRAFT 等),读出时回落到领域枚举的默认值
//! 以兜住脏数据;断言/匹配规则为 JSONB,以 `serde_json::Value` 绑定与读取。

use async_trait::async_trait;

use crate::domain::{
    ApiCase, ApiDefinition, ApiMock, ApiProtocol, ApiStatus, NewApiCase, NewApiDefinition,
    NewApiMock,
};
use crate::ports::{ApiDefinitionRepository, RepoError};
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
        project_id: row.try_get("project_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        protocol: ApiProtocol::parse(&protocol).unwrap_or_default(),
        method: row.try_get("method").map_err(map_err)?,
        path: row.try_get("path").map_err(map_err)?,
        status: ApiStatus::parse(&status).unwrap_or_default(),
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
    })
}

#[async_trait]
impl ApiDefinitionRepository for PgApiDefinitionRepository {
    async fn insert_definition(
        &self,
        d: &NewApiDefinition,
    ) -> Result<ApiDefinition, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_definition (project_id, name, protocol, method, path, status) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, project_id, name, protocol, method, path, status",
        )
        .bind(&d.project_id)
        .bind(&d.name)
        .bind(d.protocol.as_str())
        .bind(&d.method)
        .bind(&d.path)
        .bind(d.status.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_definition(&row)
    }

    async fn get_definition(&self, id: &str) -> Result<Option<ApiDefinition>, RepoError> {
        let row = sqlx::query(
            "SELECT id, project_id, name, protocol, method, path, status \
             FROM ms_api_definition WHERE id = $1 AND deleted = false",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        row.as_ref().map(row_to_definition).transpose()
    }

    async fn list_definitions(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiDefinition>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, project_id, name, protocol, method, path, status \
             FROM ms_api_definition WHERE project_id = $1 AND deleted = false",
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
                (api_definition_id, project_id, name, method, url, body, assertions) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id, api_definition_id, project_id, name, method, url, body, assertions",
        )
        .bind(&c.api_definition_id)
        .bind(&c.project_id)
        .bind(&c.name)
        .bind(&c.method)
        .bind(&c.url)
        .bind(&c.body)
        .bind(&c.assertions)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_case(&row)
    }

    async fn list_cases(&self, api_definition_id: &str) -> Result<Vec<ApiCase>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, api_definition_id, project_id, name, method, url, body, assertions \
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
            "SELECT id, api_definition_id, project_id, name, method, url, body, assertions \
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
                (api_definition_id, name, match_rule, response_status, response_body, enabled) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, api_definition_id, name, match_rule, response_status, response_body, enabled",
        )
        .bind(&m.api_definition_id)
        .bind(&m.name)
        .bind(&m.match_rule)
        .bind(m.response_status)
        .bind(&m.response_body)
        .bind(m.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_mock(&row)
    }

    async fn list_mocks(&self, api_definition_id: &str) -> Result<Vec<ApiMock>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, api_definition_id, name, match_rule, response_status, response_body, enabled \
             FROM ms_api_mock WHERE api_definition_id = $1 AND deleted = false",
        )
        .bind(api_definition_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_mock).collect()
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

        // 接口定义往返
        let nd = NewApiDefinition::new("p1", "登录", ApiProtocol::Http, "POST", "/login")
            .expect("valid");
        let def = repo.insert_definition(&nd).await.expect("insert def");
        assert_eq!(def.method, "POST");
        assert_eq!(repo.get_definition(&def.id).await.expect("get").expect("some").name, "登录");
        assert_eq!(repo.list_definitions("p1").await.expect("list").len(), 1);
        assert!(repo.get_definition("ghost").await.expect("get").is_none());

        // 用例往返
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

        // 项目级用例分页(含上面这条 def 用例 + 一条独立用例)
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

        // Mock 往返
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
    }
}

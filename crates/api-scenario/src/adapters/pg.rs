//! PostgreSQL 实现的 `ApiScenarioRepository`。
//!
//! 表结构(已迁移):
//! - `ms_api_scenario(id, project_id, name, status, deleted)`
//! - `ms_api_scenario_step(id, scenario_id, step_order, kind, ref_mode, ref_id NULL, inline JSONB NULL)`
//!
//! 步骤落库:REQUEST → kind='REQUEST',InlineRequest JSON 存 inline;
//! CASE → kind='CASE',ref_id=case_id;SCENARIO → kind='SCENARIO',ref_id=scenario_id;
//! COPY 快照存 inline。get_scenario 按 step_order 加载,并据 kind+ref_id+inline 重建 StepKind。

use async_trait::async_trait;

use crate::domain::{
    ApiScenario, InlineRequest, NewApiScenario, NewScenarioStep, RefMode, ScenarioStatus,
    ScenarioStep, StepKind,
};
use crate::ports::{ApiScenarioRepository, RepoError};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgApiScenarioRepository {
    pool: PgPool,
}

impl PgApiScenarioRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

/// 由场景行重建 `ApiScenario`(steps 由调用方另行填充)。
fn row_to_scenario(row: &sqlx::postgres::PgRow) -> Result<ApiScenario, RepoError> {
    let status: String = row.try_get("status").map_err(map_err)?;
    Ok(ApiScenario {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        status: ScenarioStatus::parse(&status),
        steps: Vec::new(),
    })
}

/// 由步骤行(kind + ref_id + inline)重建 `ScenarioStep`。
fn row_to_step(row: &sqlx::postgres::PgRow) -> Result<ScenarioStep, RepoError> {
    let id: String = row.try_get("id").map_err(map_err)?;
    let order: i32 = row.try_get("step_order").map_err(map_err)?;
    let kind_s: String = row.try_get("kind").map_err(map_err)?;
    let ref_mode_s: String = row.try_get("ref_mode").map_err(map_err)?;
    let ref_id: Option<String> = row.try_get("ref_id").map_err(map_err)?;
    let inline: Option<serde_json::Value> = row.try_get("inline").map_err(map_err)?;

    let kind = match kind_s.as_str() {
        "REQUEST" => {
            // inline 即 InlineRequest 的 JSON。
            let v = inline.clone().ok_or_else(|| {
                RepoError::Backend("REQUEST step missing inline payload".into())
            })?;
            let method = v.get("method").and_then(|m| m.as_str()).unwrap_or_default();
            let url = v.get("url").and_then(|u| u.as_str()).unwrap_or_default();
            let body = v.get("body").and_then(|b| b.as_str()).map(|s| s.to_string());
            let req = InlineRequest::new(method, url, body)
                .map_err(|e| RepoError::Backend(e.to_string()))?;
            StepKind::Request(req)
        }
        "CASE" => StepKind::Case {
            case_id: ref_id.clone().ok_or_else(|| {
                RepoError::Backend("CASE step missing ref_id".into())
            })?,
        },
        "SCENARIO" => StepKind::Scenario {
            scenario_id: ref_id.clone().ok_or_else(|| {
                RepoError::Backend("SCENARIO step missing ref_id".into())
            })?,
        },
        other => return Err(RepoError::Backend(format!("unknown step kind: {other}"))),
    };

    // COPY 模式的快照即 inline(REQUEST 已把 inline 消费为请求体,这里对非 REQUEST 保留)。
    let snapshot = match &kind {
        StepKind::Request(_) => None,
        _ => inline,
    };

    Ok(ScenarioStep { id, order, kind, ref_mode: RefMode::parse(&ref_mode_s), snapshot })
}

impl PgApiScenarioRepository {
    /// 加载某场景的步骤,按 step_order 升序。
    async fn load_steps(&self, scenario_id: &str) -> Result<Vec<ScenarioStep>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, step_order, kind, ref_mode, ref_id, inline \
             FROM ms_api_scenario_step WHERE scenario_id = $1 ORDER BY step_order",
        )
        .bind(scenario_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_step).collect()
    }
}

#[async_trait]
impl ApiScenarioRepository for PgApiScenarioRepository {
    async fn insert_scenario(
        &self,
        s: &NewApiScenario,
    ) -> Result<ApiScenario, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_api_scenario (project_id, name, status) VALUES ($1, $2, $3) \
             RETURNING id, project_id, name, status, deleted",
        )
        .bind(&s.project_id)
        .bind(&s.name)
        .bind(ScenarioStatus::Draft.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_scenario(&row)
    }

    async fn get_scenario(&self, id: &str) -> Result<Option<ApiScenario>, RepoError> {
        let row = sqlx::query(
            "SELECT id, project_id, name, status, deleted FROM ms_api_scenario \
             WHERE id = $1 AND deleted = false",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        let Some(row) = row else { return Ok(None) };
        let mut scenario = row_to_scenario(&row)?;
        scenario.steps = self.load_steps(&scenario.id).await?;
        Ok(Some(scenario))
    }

    async fn list_scenarios(
        &self,
        project_id: &str,
    ) -> Result<Vec<ApiScenario>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, project_id, name, status, deleted FROM ms_api_scenario \
             WHERE project_id = $1 AND deleted = false ORDER BY id",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut scenario = row_to_scenario(row)?;
            scenario.steps = self.load_steps(&scenario.id).await?;
            out.push(scenario);
        }
        Ok(out)
    }

    async fn add_step(
        &self,
        scenario_id: &str,
        step: &NewScenarioStep,
    ) -> Result<ScenarioStep, RepoError> {
        // 据步骤类型拆出 ref_id 与 inline 两列。
        let (ref_id, inline): (Option<String>, Option<serde_json::Value>) = match &step.kind {
            StepKind::Request(req) => {
                let mut v = serde_json::json!({ "method": req.method, "url": req.url });
                if let Some(b) = &req.body {
                    v["body"] = serde_json::Value::String(b.clone());
                }
                (None, Some(v))
            }
            StepKind::Case { case_id } => (Some(case_id.clone()), step.snapshot.clone()),
            StepKind::Scenario { scenario_id } => {
                (Some(scenario_id.clone()), step.snapshot.clone())
            }
        };

        let row = sqlx::query(
            "INSERT INTO ms_api_scenario_step \
                (scenario_id, step_order, kind, ref_mode, ref_id, inline) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, step_order, kind, ref_mode, ref_id, inline",
        )
        .bind(scenario_id)
        .bind(step.order)
        .bind(step.kind.kind_str())
        .bind(step.ref_mode.as_str())
        .bind(ref_id)
        .bind(inline)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_step(&row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_scenario_steps_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_api_scenario, ms_api_scenario_step")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgApiScenarioRepository::new(pool.clone());

        let scenario = repo
            .insert_scenario(&NewApiScenario::new("p1", "下单链路").expect("valid"))
            .await
            .expect("insert");
        assert_eq!(scenario.status, ScenarioStatus::Draft);

        // 三类步骤各加一个(顺序故意打乱)。
        let req = InlineRequest::new("POST", "http://x/order", Some("{}".into())).expect("valid");
        repo.add_step(
            &scenario.id,
            &NewScenarioStep::new(2, StepKind::Request(req), RefMode::Reference, None).expect("valid"),
        )
        .await
        .expect("req step");
        repo.add_step(
            &scenario.id,
            &NewScenarioStep::new(
                0,
                StepKind::Case { case_id: "case-1".into() },
                RefMode::Reference,
                None,
            )
            .expect("valid"),
        )
        .await
        .expect("case step");
        repo.add_step(
            &scenario.id,
            &NewScenarioStep::new(
                1,
                StepKind::Scenario { scenario_id: "scn-x".into() },
                RefMode::Copy,
                Some(serde_json::json!({"snap": true})),
            )
            .expect("valid"),
        )
        .await
        .expect("scenario step");

        let loaded = repo.get_scenario(&scenario.id).await.expect("get").expect("some");
        // 按 order 升序:CASE(0) → SCENARIO(1) → REQUEST(2)
        let kinds: Vec<_> = loaded.steps.iter().map(|s| s.kind.kind_str()).collect();
        assert_eq!(kinds, vec!["CASE", "SCENARIO", "REQUEST"]);
        assert_eq!(loaded.steps[0].order, 0);
        match &loaded.steps[0].kind {
            StepKind::Case { case_id } => assert_eq!(case_id, "case-1"),
            other => panic!("expected CASE, got {other:?}"),
        }
        match &loaded.steps[2].kind {
            StepKind::Request(r) => assert_eq!(r.url, "http://x/order"),
            other => panic!("expected REQUEST, got {other:?}"),
        }

        // 列表与 get 一致。
        let list = repo.list_scenarios("p1").await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].steps.len(), 3);
    }
}

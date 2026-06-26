use async_trait::async_trait;
use crate::domain::{
    AssertionResult, CaseCounts, CaseResult, CaseStatus, NewPlan, Plan, PlanCase, PlanType,
    RequestInfo, StepResult,
};
use crate::ports::{PlanRepository, RepoError};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PgPlanRepository {
    pool: PgPool,
}

impl PgPlanRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn row_to_plan(row: &sqlx::postgres::PgRow) -> Result<Plan, RepoError> {
    let type_raw: String = row.try_get("plan_type").map_err(map_err)?;
    let plan_type = PlanType::parse(&type_raw)
        .ok_or_else(|| RepoError::Backend(format!("bad plan_type: {type_raw}")))?;
    Ok(Plan {
        id: row.try_get("id").map_err(map_err)?,
        project_id: row.try_get("project_id").map_err(map_err)?,
        name: row.try_get("name").map_err(map_err)?,
        plan_type,
        group_id: row.try_get("group_id").map_err(map_err)?,
        archived: row.try_get("archived").map_err(map_err)?,
        created_at_ms: row.try_get("created_at_ms").map_err(map_err)?,
    })
}

const PLAN_COLS: &str =
    "id, project_id, name, plan_type, group_id, archived, (extract(epoch from created_at)*1000)::bigint AS created_at_ms";

#[async_trait]
impl PlanRepository for PgPlanRepository {
    async fn insert(&self, new_plan: &NewPlan) -> Result<Plan, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_test_plan (project_id, name, plan_type, group_id) \
             VALUES ($1, $2, $3, $4) RETURNING id, project_id, name, plan_type, group_id, archived, (extract(epoch from created_at)*1000)::bigint AS created_at_ms",
        )
        .bind(&new_plan.project_id)
        .bind(&new_plan.name)
        .bind(new_plan.plan_type.as_str())
        .bind(&new_plan.group_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_plan(&row)
    }

    async fn get(&self, id: &str) -> Result<Option<Plan>, RepoError> {
        let row = sqlx::query(&format!("SELECT {PLAN_COLS} FROM ms_test_plan WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        row.as_ref().map(row_to_plan).transpose()
    }

    async fn children(&self, group_id: &str) -> Result<Vec<Plan>, RepoError> {
        let rows =
            sqlx::query(&format!("SELECT {PLAN_COLS} FROM ms_test_plan WHERE group_id = $1 ORDER BY id"))
                .bind(group_id)
                .fetch_all(&self.pool)
                .await
                .map_err(map_err)?;
        rows.iter().map(row_to_plan).collect()
    }

    async fn case_counts(&self, plan_id: &str) -> Result<CaseCounts, RepoError> {
        let rows = sqlx::query(
            "SELECT status, count(*)::bigint AS n FROM ms_test_plan_case \
             WHERE plan_id = $1 GROUP BY status",
        )
        .bind(plan_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut c = CaseCounts::default();
        for r in &rows {
            let status: String = r.try_get("status").map_err(map_err)?;
            let n = r.try_get::<i64, _>("n").map_err(map_err)?.max(0) as u64;
            match CaseStatus::parse(&status) {
                Some(CaseStatus::Pending) => c.pending += n,
                Some(CaseStatus::Success) => c.success += n,
                Some(CaseStatus::Error) => c.error += n,
                Some(CaseStatus::FakeError) => c.fake_error += n,
                Some(CaseStatus::Block) => c.block += n,
                None => {}
            }
        }
        Ok(c)
    }

    async fn pass_threshold(&self, plan_id: &str) -> Result<f64, RepoError> {
        let row = sqlx::query("SELECT pass_threshold FROM ms_test_plan WHERE id = $1")
            .bind(plan_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        match row {
            Some(r) => Ok(r.try_get::<f64, _>("pass_threshold").map_err(map_err)?),
            None => Ok(0.0),
        }
    }

    async fn link_case(&self, plan_id: &str, case_id: &str, name: &str) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_test_plan_case (plan_id, case_id, name, status) \
             VALUES ($1, $2, $3, 'PENDING') ON CONFLICT (plan_id, case_id) DO NOTHING",
        )
        .bind(plan_id)
        .bind(case_id)
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn record_result(
        &self,
        plan_id: &str,
        case_id: &str,
        status: CaseStatus,
        result: Option<&CaseResult>,
    ) -> Result<bool, RepoError> {
        let result_json = result.map(result_to_json);
        let res = sqlx::query(
            "UPDATE ms_test_plan_case SET status = $3, result = $4, executed_at = now() \
             WHERE plan_id = $1 AND case_id = $2",
        )
        .bind(plan_id)
        .bind(case_id)
        .bind(status.as_str())
        .bind(result_json)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn list_cases(&self, plan_id: &str) -> Result<Vec<PlanCase>, RepoError> {
        let rows = sqlx::query(
            "SELECT case_id, name, status, result FROM ms_test_plan_case \
             WHERE plan_id = $1 ORDER BY executed_at NULLS LAST, name, case_id",
        )
        .bind(plan_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                let status: String = r.try_get("status").map_err(map_err)?;
                let result_json: Option<serde_json::Value> = r.try_get("result").map_err(map_err)?;
                Ok(PlanCase {
                    case_id: r.try_get("case_id").map_err(map_err)?,
                    name: r.try_get("name").map_err(map_err)?,
                    status: CaseStatus::parse(&status).unwrap_or(CaseStatus::Pending),
                    result: result_json.map(|v| result_from_json(&v)),
                })
            })
            .collect()
    }

    async fn list(&self, project_id: &str) -> Result<Vec<Plan>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {PLAN_COLS} FROM ms_test_plan WHERE project_id = $1 AND NOT archived ORDER BY name, id"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_plan).collect()
    }

    async fn rename(&self, id: &str, name: &str) -> Result<bool, RepoError> {
        let res = sqlx::query("UPDATE ms_test_plan SET name = $2 WHERE id = $1")
            .bind(id)
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        // 无 FK 级联:必须先删挂入用例,再删计划本体。
        sqlx::query("DELETE FROM ms_test_plan_case WHERE plan_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        let res = sqlx::query("DELETE FROM ms_test_plan WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
    }
}

fn assertions_to_json(a: &[AssertionResult]) -> serde_json::Value {
    serde_json::Value::Array(
        a.iter()
            .map(|a| serde_json::json!({
                "item": a.item, "actual": a.actual, "condition": a.condition,
                "expected": a.expected, "passed": a.passed, "reason": a.reason,
            }))
            .collect(),
    )
}

fn headers_to_json(h: &[(String, String)]) -> serde_json::Value {
    serde_json::Value::Array(
        h.iter().map(|(n, v)| serde_json::json!({"name": n, "value": v})).collect(),
    )
}

fn step_to_json(s: &StepResult) -> serde_json::Value {
    serde_json::json!({
        "name": s.name, "kind": s.kind, "status": s.status.as_str(),
        "latencyMs": s.latency_ms, "statusCode": s.status_code,
        "assertions": assertions_to_json(&s.assertions),
        "children": s.children.iter().map(step_to_json).collect::<Vec<_>>(),
    })
}

fn result_to_json(r: &CaseResult) -> serde_json::Value {
    let request = r.request.as_ref().map(|req| {
        serde_json::json!({
            "method": req.method, "url": req.url, "body": req.body,
            "headers": headers_to_json(&req.headers),
        })
    });
    serde_json::json!({
        "latencyMs": r.latency_ms,
        "responseSize": r.response_size,
        "statusCode": r.status_code,
        "body": r.body,
        "assertions": assertions_to_json(&r.assertions),
        "responseHeaders": headers_to_json(&r.response_headers),
        "request": request,
        "steps": r.steps.iter().map(step_to_json).collect::<Vec<_>>(),
    })
}

fn assertions_from_json(v: &serde_json::Value) -> Vec<AssertionResult> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| AssertionResult {
                    item: a.get("item").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                    actual: a.get("actual").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                    condition: a.get("condition").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                    expected: a.get("expected").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                    passed: a.get("passed").and_then(|x| x.as_bool()).unwrap_or(false),
                    reason: a.get("reason").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn headers_from_json(v: &serde_json::Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .map(|h| {
                    (
                        h.get("name").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                        h.get("value").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn step_from_json(v: &serde_json::Value) -> StepResult {
    StepResult {
        name: v.get("name").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        kind: v.get("kind").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        status: v
            .get("status")
            .and_then(|x| x.as_str())
            .and_then(CaseStatus::parse)
            .unwrap_or(CaseStatus::Pending),
        latency_ms: v.get("latencyMs").and_then(|x| x.as_u64()).unwrap_or(0),
        status_code: v.get("statusCode").and_then(|x| x.as_i64()),
        assertions: v.get("assertions").map(assertions_from_json).unwrap_or_default(),
        children: v
            .get("children")
            .and_then(|c| c.as_array())
            .map(|arr| arr.iter().map(step_from_json).collect())
            .unwrap_or_default(),
    }
}

fn result_from_json(v: &serde_json::Value) -> CaseResult {
    let opt_str = |k: &str| {
        let s = v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
        if s.is_empty() { None } else { Some(s) }
    };
    let request = v.get("request").filter(|r| r.is_object()).map(|r| RequestInfo {
        method: r.get("method").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        url: r.get("url").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        headers: r.get("headers").map(headers_from_json).unwrap_or_default(),
        body: {
            let b = r.get("body").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            if b.is_empty() { None } else { Some(b) }
        },
    });
    CaseResult {
        latency_ms: v.get("latencyMs").and_then(|x| x.as_u64()).unwrap_or(0),
        response_size: v.get("responseSize").and_then(|x| x.as_u64()).unwrap_or(0),
        status_code: v.get("statusCode").and_then(|x| x.as_i64()),
        assertions: v.get("assertions").map(assertions_from_json).unwrap_or_default(),
        body: opt_str("body"),
        response_headers: v.get("responseHeaders").map(headers_from_json).unwrap_or_default(),
        request,
        steps: v
            .get("steps")
            .and_then(|s| s.as_array())
            .map(|arr| arr.iter().map(step_from_json).collect())
            .unwrap_or_default(),
    }
}

#[derive(Clone)]
pub struct PgScheduleStore {
    pool: PgPool,
}

impl PgScheduleStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl crate::ports::ScheduleStore for PgScheduleStore {
    async fn insert(
        &self,
        s: &crate::domain::NewSchedule,
    ) -> Result<crate::domain::Schedule, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_test_plan_schedule (plan_id, cron, enabled) VALUES ($1,$2,$3) \
             RETURNING id, plan_id, cron, enabled",
        )
        .bind(&s.plan_id)
        .bind(&s.cron)
        .bind(s.enabled)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(crate::domain::Schedule {
            id: row.try_get("id").map_err(map_err)?,
            plan_id: row.try_get("plan_id").map_err(map_err)?,
            cron: row.try_get("cron").map_err(map_err)?,
            enabled: row.try_get("enabled").map_err(map_err)?,
        })
    }

    async fn list_enabled(&self) -> Result<Vec<crate::domain::Schedule>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, plan_id, cron, enabled FROM ms_test_plan_schedule \
             WHERE enabled AND NOT deleted ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.into_iter()
            .map(|r| {
                Ok(crate::domain::Schedule {
                    id: r.try_get("id").map_err(map_err)?,
                    plan_id: r.try_get("plan_id").map_err(map_err)?,
                    cron: r.try_get("cron").map_err(map_err)?,
                    enabled: r.try_get("enabled").map_err(map_err)?,
                })
            })
            .collect()
    }
}

#[async_trait]
impl crate::ports::PlanRunStore for PgScheduleStore {
    async fn record(
        &self,
        plan_id: &str,
        status: &str,
        total: u64,
        pass_rate: f64,
        execute_rate: f64,
    ) -> Result<crate::domain::PlanRun, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_test_plan_run (plan_id, status, total, pass_rate, execute_rate) \
             VALUES ($1,$2,$3,$4,$5) RETURNING id, plan_id, status, total, pass_rate, execute_rate, \
             to_char(triggered_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS triggered_at",
        )
        .bind(plan_id)
        .bind(status)
        .bind(total as i64)
        .bind(pass_rate)
        .bind(execute_rate)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        row_to_run(&row)
    }

    async fn list_by_plan(
        &self,
        plan_id: &str,
        limit: u32,
    ) -> Result<Vec<crate::domain::PlanRun>, RepoError> {
        let rows = sqlx::query(
            "SELECT id, plan_id, status, total, pass_rate, execute_rate, \
             to_char(triggered_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS triggered_at \
             FROM ms_test_plan_run WHERE plan_id = $1 ORDER BY triggered_at DESC LIMIT $2",
        )
        .bind(plan_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_run).collect()
    }
}

fn row_to_run(r: &sqlx::postgres::PgRow) -> Result<crate::domain::PlanRun, RepoError> {
    Ok(crate::domain::PlanRun {
        id: r.try_get("id").map_err(map_err)?,
        plan_id: r.try_get("plan_id").map_err(map_err)?,
        status: r.try_get("status").map_err(map_err)?,
        total: r.try_get::<i64, _>("total").map_err(map_err)? as u64,
        pass_rate: r.try_get("pass_rate").map_err(map_err)?,
        execute_rate: r.try_get("execute_rate").map_err(map_err)?,
        triggered_at: r.try_get("triggered_at").map_err(map_err)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_plan_group_children_and_counts() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::raw_sql("TRUNCATE ms_test_plan, ms_test_plan_case")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgPlanRepository::new(pool.clone());

        let group = repo
            .insert(&NewPlan::new("proj1", "组", PlanType::Group, "NONE").expect("v"))
            .await
            .expect("group");
        let child = repo
            .insert(&NewPlan::new("proj1", "子", PlanType::Plan, &group.id).expect("v"))
            .await
            .expect("child");

        let got = repo.get(&child.id).await.expect("get").expect("some");
        assert_eq!(got.plan_type, PlanType::Plan);
        assert_eq!(got.group_id, group.id);

        let kids = repo.children(&group.id).await.expect("children");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].id, child.id);

        assert_eq!(repo.case_counts(&child.id).await.expect("c"), CaseCounts::default());
        repo.link_case(&child.id, "ca", "用例A").await.expect("link a");
        repo.link_case(&child.id, "cb", "用例B").await.expect("link b");
        repo.link_case(&child.id, "cc", "用例C").await.expect("link c");
        repo.link_case(&child.id, "ca", "用例A").await.expect("link a again");
        let res = CaseResult {
            latency_ms: 12,
            response_size: 3,
            status_code: Some(200),
            assertions: vec![AssertionResult {
                item: "状态码".into(),
                actual: "200".into(),
                condition: "等于".into(),
                expected: "200".into(),
                passed: true,
                reason: String::new(),
            }],
            body: Some("ok".into()),
            ..Default::default()
        };
        assert!(repo.record_result(&child.id, "ca", CaseStatus::Success, Some(&res)).await.expect("rec a"));
        assert!(repo.record_result(&child.id, "cb", CaseStatus::Success, None).await.expect("rec b"));
        assert!(!repo.record_result(&child.id, "ghost", CaseStatus::Success, None).await.expect("rec ghost"));

        let c = repo.case_counts(&child.id).await.expect("c");
        assert_eq!(c.success, 2);
        assert_eq!(c.pending, 1);
        assert_eq!(c.total(), 3);

        let cases = repo.list_cases(&child.id).await.expect("list");
        assert_eq!(cases.len(), 3);
        let ca = cases.iter().find(|x| x.case_id == "ca").expect("ca");
        assert_eq!(ca.name, "用例A");
        assert_eq!(ca.status, CaseStatus::Success);
        let r = ca.result.as_ref().expect("result");
        assert_eq!(r.latency_ms, 12);
        assert_eq!(r.status_code, Some(200));
        assert_eq!(r.assertions.len(), 1);
        assert!(r.assertions[0].passed);
        assert_eq!(r.body.as_deref(), Some("ok"));

        assert_eq!(repo.pass_threshold(&child.id).await.expect("t"), 1.0);
    }
}

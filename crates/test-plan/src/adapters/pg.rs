//! PostgreSQL 实现的 `PlanRepository`。

use async_trait::async_trait;
use crate::domain::{CaseCounts, NewPlan, Plan, PlanType};
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
    })
}

const PLAN_COLS: &str = "id, project_id, name, plan_type, group_id, archived";

#[async_trait]
impl PlanRepository for PgPlanRepository {
    async fn insert(&self, new_plan: &NewPlan) -> Result<Plan, RepoError> {
        let row = sqlx::query(
            "INSERT INTO ms_test_plan (project_id, name, plan_type, group_id) \
             VALUES ($1, $2, $3, $4) RETURNING id, project_id, name, plan_type, group_id, archived",
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
        let row = sqlx::query(
            "SELECT pending, success, error, fake_error, block \
             FROM ms_test_plan_case_count WHERE plan_id = $1",
        )
        .bind(plan_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;

        let Some(r) = row else { return Ok(CaseCounts::default()) };
        let get = |name: &str| -> Result<u64, RepoError> {
            let v: i64 = r.try_get(name).map_err(map_err)?;
            Ok(v.max(0) as u64)
        };
        Ok(CaseCounts {
            pending: get("pending")?,
            success: get("success")?,
            error: get("error")?,
            fake_error: get("fake_error")?,
            block: get("block")?,
        })
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
        sqlx::raw_sql("TRUNCATE ms_test_plan, ms_test_plan_case_count")
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

        // get 往返 + 类型解析
        let got = repo.get(&child.id).await.expect("get").expect("some");
        assert_eq!(got.plan_type, PlanType::Plan);
        assert_eq!(got.group_id, group.id);

        // children
        let kids = repo.children(&group.id).await.expect("children");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].id, child.id);

        // case_counts:无记录默认 0;写入后读回
        assert_eq!(repo.case_counts(&child.id).await.expect("c"), CaseCounts::default());
        sqlx::query(
            "INSERT INTO ms_test_plan_case_count (plan_id, pending, success) VALUES ($1, 1, 2)",
        )
        .bind(&child.id)
        .execute(&pool)
        .await
        .expect("seed counts");
        let c = repo.case_counts(&child.id).await.expect("c");
        assert_eq!(c.pending, 1);
        assert_eq!(c.success, 2);

        // pass_threshold 默认 1.0
        assert_eq!(repo.pass_threshold(&child.id).await.expect("t"), 1.0);
    }
}

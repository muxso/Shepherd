//! PostgreSQL 实现的 `DeliveryRepository`。表 `ms_delivery_attempt`(交付物字段平铺、可空)。
//!
//! 集成测试 `#[ignore]`,需 DATABASE_URL:
//!   `DATABASE_URL=postgres://... cargo test -p delivery --features pg -- --ignored`

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{
    AttemptStatus, Deliverable, DeliverableKind, DeliveryAttempt, ExecutorKind,
};
use crate::ports::{DeliveryRepository, RepoError};

#[derive(Clone)]
pub struct PgDeliveryRepository {
    pool: PgPool,
}

impl PgDeliveryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

const COLS: &str = "id, decomposition_id, task_id, executor, status, run_id, \
                    deliverable_kind, deliverable_reference, deliverable_summary, error";

fn row_to_attempt(row: &sqlx::postgres::PgRow) -> Result<DeliveryAttempt, RepoError> {
    let executor_s: String = row.try_get("executor").map_err(map_err)?;
    let status_s: String = row.try_get("status").map_err(map_err)?;
    let dkind: Option<String> = row.try_get("deliverable_kind").map_err(map_err)?;
    let dref: Option<String> = row.try_get("deliverable_reference").map_err(map_err)?;
    let dsum: Option<String> = row.try_get("deliverable_summary").map_err(map_err)?;

    let deliverable = match (dkind.as_deref().and_then(DeliverableKind::parse), dref, dsum) {
        (Some(kind), Some(reference), summary) => {
            Some(Deliverable { kind, reference, summary: summary.unwrap_or_default() })
        }
        _ => None,
    };

    Ok(DeliveryAttempt {
        id: row.try_get("id").map_err(map_err)?,
        decomposition_id: row.try_get("decomposition_id").map_err(map_err)?,
        task_id: row.try_get("task_id").map_err(map_err)?,
        executor: ExecutorKind::parse(&executor_s).unwrap_or(ExecutorKind::ClaudeCode),
        status: AttemptStatus::parse(&status_s).unwrap_or(AttemptStatus::Dispatched),
        run_id: row.try_get("run_id").map_err(map_err)?,
        deliverable,
        error: row.try_get("error").map_err(map_err)?,
    })
}

#[async_trait]
impl DeliveryRepository for PgDeliveryRepository {
    async fn create(
        &self,
        decomposition_id: &str,
        task_id: &str,
        executor: ExecutorKind,
    ) -> Result<DeliveryAttempt, RepoError> {
        let id: String = sqlx::query(
            "INSERT INTO ms_delivery_attempt (decomposition_id, task_id, executor, status) \
             VALUES ($1, $2, $3, 'DISPATCHED') RETURNING id",
        )
        .bind(decomposition_id)
        .bind(task_id)
        .bind(executor.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?
        .try_get("id")
        .map_err(map_err)?;
        Ok(DeliveryAttempt::dispatched(&id, decomposition_id, task_id, executor))
    }

    async fn get(&self, id: &str) -> Result<Option<DeliveryAttempt>, RepoError> {
        let row = sqlx::query(&format!("SELECT {COLS} FROM ms_delivery_attempt WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        row.as_ref().map(row_to_attempt).transpose()
    }

    async fn list_by_task(
        &self,
        decomposition_id: &str,
        task_id: &str,
    ) -> Result<Vec<DeliveryAttempt>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLS} FROM ms_delivery_attempt \
             WHERE decomposition_id = $1 AND task_id = $2 ORDER BY seq"
        ))
        .bind(decomposition_id)
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter().map(row_to_attempt).collect()
    }

    async fn save(&self, attempt: &DeliveryAttempt) -> Result<(), RepoError> {
        let (dkind, dref, dsum) = match &attempt.deliverable {
            Some(d) => (Some(d.kind.as_str()), Some(d.reference.clone()), Some(d.summary.clone())),
            None => (None, None, None),
        };
        sqlx::query(
            "UPDATE ms_delivery_attempt SET status = $2, run_id = $3, \
             deliverable_kind = $4, deliverable_reference = $5, deliverable_summary = $6, error = $7 \
             WHERE id = $1",
        )
        .bind(&attempt.id)
        .bind(attempt.status.as_str())
        .bind(&attempt.run_id)
        .bind(dkind)
        .bind(dref)
        .bind(dsum)
        .bind(&attempt.error)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DeliverableKind;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_attempt_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_delivery_attempt").execute(&pool).await.expect("truncate");

        let repo = PgDeliveryRepository::new(pool.clone());
        let mut a = repo.create("d1", "t1", ExecutorKind::ClaudeCode).await.expect("create");
        a.start_running("run-9").expect("run");
        a.deliver(Deliverable {
            kind: DeliverableKind::PullRequest,
            reference: "pr/42".into(),
            summary: "done".into(),
        })
        .expect("deliver");
        repo.save(&a).await.expect("save");

        let got = repo.get(&a.id).await.expect("get").expect("some");
        assert_eq!(got.status, AttemptStatus::Delivered);
        assert_eq!(got.run_id.as_deref(), Some("run-9"));
        assert_eq!(got.deliverable.expect("d").reference, "pr/42");
        assert_eq!(repo.list_by_task("d1", "t1").await.expect("list").len(), 1);
    }
}

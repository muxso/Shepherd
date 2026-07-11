use async_trait::async_trait;
use sqlx::{PgPool, QueryBuilder, Row};

use crate::domain::{
    AttemptStatus, Deliverable, DeliverableKind, DeliveryAttempt, EventKind, ExecutionEvent,
    ExecutorKind, NewExecutionEvent,
};
use crate::ports::{
    CollabDay, CollabRequirementRow, CollabStats, DeliveryRepository, RepoError, TaskListFilter,
    TaskPage, TaskRow,
};

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

const COLS: &str = "id, decomposition_id, task_id, executor, target_runtime, status, run_id, \
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
        target_runtime: row.try_get("target_runtime").map_err(map_err)?,
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
        target_runtime: Option<&str>,
    ) -> Result<DeliveryAttempt, RepoError> {
        let id: String = sqlx::query(
            "INSERT INTO ms_delivery_attempt (decomposition_id, task_id, executor, target_runtime, status) \
             VALUES ($1, $2, $3, $4, 'DISPATCHED') RETURNING id",
        )
        .bind(decomposition_id)
        .bind(task_id)
        .bind(executor.as_str())
        .bind(target_runtime)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?
        .try_get("id")
        .map_err(map_err)?;
        Ok(DeliveryAttempt::dispatched(&id, decomposition_id, task_id, executor, target_runtime))
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

    async fn append_event(
        &self,
        attempt_id: &str,
        event: &NewExecutionEvent,
    ) -> Result<ExecutionEvent, RepoError> {
        let seq: i64 = sqlx::query(
            "INSERT INTO ms_delivery_event (attempt_id, kind, message, detail) \
             VALUES ($1, $2, $3, $4) RETURNING seq",
        )
        .bind(attempt_id)
        .bind(event.kind.as_str())
        .bind(&event.message)
        .bind(&event.detail)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?
        .try_get("seq")
        .map_err(map_err)?;
        Ok(ExecutionEvent {
            seq,
            kind: event.kind,
            message: event.message.clone(),
            detail: event.detail.clone(),
        })
    }

    async fn list_events(&self, attempt_id: &str) -> Result<Vec<ExecutionEvent>, RepoError> {
        let rows = sqlx::query(
            "SELECT seq, kind, message, detail FROM ms_delivery_event \
             WHERE attempt_id = $1 ORDER BY seq",
        )
        .bind(attempt_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        rows.iter()
            .map(|r| {
                let kind_s: String = r.try_get("kind").map_err(map_err)?;
                Ok(ExecutionEvent {
                    seq: r.try_get("seq").map_err(map_err)?,
                    kind: EventKind::parse(&kind_s).unwrap_or(EventKind::Log),
                    message: r.try_get("message").map_err(map_err)?,
                    detail: r.try_get("detail").map_err(map_err)?,
                })
            })
            .collect()
    }

    async fn list_tasks(&self, filter: &TaskListFilter) -> Result<TaskPage, RepoError> {
        fn push_where<'a>(qb: &mut QueryBuilder<'a, sqlx::Postgres>, f: &'a TaskListFilter) {
            qb.push(" WHERE 1 = 1");
            if let Some(s) = &f.status {
                qb.push(" AND a.status = ").push_bind(s.as_str());
            }
            if let Some(e) = &f.executor {
                qb.push(" AND a.executor = ").push_bind(e.as_str());
            }
            if f.active_only {
                qb.push(" AND a.status IN ('DISPATCHED','RUNNING')");
            }
            if let Some(q) = &f.query {
                if !q.trim().is_empty() {
                    let like = format!("%{}%", q.trim());
                    qb.push(" AND (t.title ILIKE ")
                        .push_bind(like.clone())
                        .push(" OR a.task_id ILIKE ")
                        .push_bind(like.clone())
                        .push(" OR a.decomposition_id ILIKE ")
                        .push_bind(like)
                        .push(")");
                }
            }
        }

        let mut cq = QueryBuilder::<sqlx::Postgres>::new(
            "SELECT count(*) AS n FROM ms_delivery_attempt a \
             LEFT JOIN ms_task t ON t.decomposition_id = a.decomposition_id AND t.id = a.task_id",
        );
        push_where(&mut cq, filter);
        let total: i64 = cq
            .build()
            .fetch_one(&self.pool)
            .await
            .map_err(map_err)?
            .try_get("n")
            .map_err(map_err)?;

        let mut pq = QueryBuilder::<sqlx::Postgres>::new(
            "SELECT a.id, a.decomposition_id, a.task_id, a.executor, a.target_runtime, a.status, a.run_id, \
             a.deliverable_kind, a.deliverable_reference, a.deliverable_summary, a.error, \
             a.created_at, t.title AS task_title, t.description AS task_description, \
             r.title AS module_title, \
             (SELECT count(*) FROM ms_delivery_event e WHERE e.attempt_id = a.id) AS event_count \
             FROM ms_delivery_attempt a \
             LEFT JOIN ms_task t ON t.decomposition_id = a.decomposition_id AND t.id = a.task_id \
             LEFT JOIN ms_task_decomposition dc ON dc.id = a.decomposition_id \
             LEFT JOIN ms_requirement r ON r.id = dc.requirement_id",
        );
        push_where(&mut pq, filter);
        pq.push(" ORDER BY a.created_at DESC, a.seq DESC");
        pq.push(" LIMIT ").push_bind(filter.limit.max(0));
        pq.push(" OFFSET ").push_bind(filter.offset.max(0));

        let rows = pq.build().fetch_all(&self.pool).await.map_err(map_err)?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(TaskRow {
                    attempt: row_to_attempt(r)?,
                    title: r.try_get("task_title").map_err(map_err)?,
                    description: r.try_get("task_description").map_err(map_err)?,
                    module: r.try_get("module_title").map_err(map_err)?,
                    created_at: r.try_get("created_at").map_err(map_err)?,
                    event_count: r.try_get("event_count").map_err(map_err)?,
                })
            })
            .collect::<Result<Vec<_>, RepoError>>()?;
        Ok(TaskPage { items, total })
    }

    // 人机协同人效:VERIFIED 任务按「是否存在 DELIVERED 交付记录」拆成 AI/人工,
    // 需求维度聚合数量与工作量 + 交付质量(尝试成功率/一次通过);requirement_id 给定时只看该需求。
    async fn collab_stats(
        &self,
        project_id: &str,
        requirement_id: Option<&str>,
    ) -> Result<CollabStats, RepoError> {
        // attempt_cnt 供「一次通过」判定;质量指标(尝试维度)单独聚合后在 SQL 里 LEFT JOIN。
        const VERIFIED_SPLIT: &str = "SELECT t.points, t.verified_at, dc.requirement_id, \
                (SELECT count(*) FROM ms_delivery_attempt a \
                 WHERE a.decomposition_id = t.decomposition_id AND a.task_id = t.id) AS attempt_cnt, \
                EXISTS(SELECT 1 FROM ms_delivery_attempt a \
                       WHERE a.decomposition_id = t.decomposition_id AND a.task_id = t.id \
                         AND a.status = 'DELIVERED') AS ai \
             FROM ms_task t \
             JOIN ms_task_decomposition dc ON dc.id = t.decomposition_id \
             WHERE t.status = 'VERIFIED'";
        // 尝试维度(不限于已验收任务):某需求下全部交付尝试的 总数/成功/失败。
        const ATTEMPT_AGG: &str = "SELECT dc.requirement_id, \
                count(*) AS ai_attempts, \
                count(*) FILTER (WHERE a.status = 'DELIVERED') AS ai_delivered, \
                count(*) FILTER (WHERE a.status = 'FAILED') AS ai_failed \
             FROM ms_delivery_attempt a \
             JOIN ms_task_decomposition dc ON dc.id = a.decomposition_id \
             GROUP BY dc.requirement_id";
        // requirement_id 过滤:$2 为空串 = 不过滤(避免动态拼 SQL)。
        let req_filter = requirement_id.unwrap_or("");

        let rows = sqlx::query(&format!(
            "SELECT r.id, r.title, \
                count(x.requirement_id) FILTER (WHERE x.ai) AS ai_tasks, \
                count(x.requirement_id) FILTER (WHERE NOT x.ai) AS human_tasks, \
                COALESCE(sum(x.points) FILTER (WHERE x.ai), 0)::BIGINT AS ai_points, \
                COALESCE(sum(x.points) FILTER (WHERE NOT x.ai), 0)::BIGINT AS human_points, \
                count(x.requirement_id) FILTER (WHERE x.ai AND x.attempt_cnt = 1) AS ai_first_pass, \
                COALESCE(max(att.ai_attempts), 0) AS ai_attempts, \
                COALESCE(max(att.ai_delivered), 0) AS ai_delivered, \
                COALESCE(max(att.ai_failed), 0) AS ai_failed \
             FROM ms_requirement r \
             LEFT JOIN ({VERIFIED_SPLIT}) x ON x.requirement_id = r.id \
             LEFT JOIN ({ATTEMPT_AGG}) att ON att.requirement_id = r.id \
             WHERE r.project_id = $1 AND ($2 = '' OR r.id = $2) \
             GROUP BY r.id, r.title \
             HAVING count(x.requirement_id) > 0 OR COALESCE(max(att.ai_attempts), 0) > 0 \
             ORDER BY count(x.requirement_id) DESC, r.id"
        ))
        .bind(project_id)
        .bind(req_filter)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let items = rows
            .iter()
            .map(|r| {
                Ok(CollabRequirementRow {
                    requirement_id: r.try_get("id").map_err(map_err)?,
                    title: r.try_get("title").map_err(map_err)?,
                    ai_tasks: r.try_get("ai_tasks").map_err(map_err)?,
                    human_tasks: r.try_get("human_tasks").map_err(map_err)?,
                    ai_points: r.try_get("ai_points").map_err(map_err)?,
                    human_points: r.try_get("human_points").map_err(map_err)?,
                    ai_attempts: r.try_get("ai_attempts").map_err(map_err)?,
                    ai_delivered: r.try_get("ai_delivered").map_err(map_err)?,
                    ai_failed: r.try_get("ai_failed").map_err(map_err)?,
                    ai_first_pass: r.try_get("ai_first_pass").map_err(map_err)?,
                })
            })
            .collect::<Result<Vec<_>, RepoError>>()?;

        // 近一年按日聚合(GitHub 贡献格子);稀疏返回,前端补全日历格。
        let rows = sqlx::query(&format!(
            "SELECT to_char(date_trunc('day', x.verified_at), 'YYYY-MM-DD') AS d, \
                count(*) FILTER (WHERE x.ai) AS ai, \
                count(*) FILTER (WHERE NOT x.ai) AS human \
             FROM ({VERIFIED_SPLIT}) x \
             JOIN ms_requirement r ON r.id = x.requirement_id \
             WHERE r.project_id = $1 AND ($2 = '' OR r.id = $2) \
               AND x.verified_at >= now() - interval '370 days' \
             GROUP BY d ORDER BY d"
        ))
        .bind(project_id)
        .bind(req_filter)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let daily = rows
            .iter()
            .map(|r| {
                Ok(CollabDay {
                    date: r.try_get("d").map_err(map_err)?,
                    ai: r.try_get("ai").map_err(map_err)?,
                    human: r.try_get("human").map_err(map_err)?,
                })
            })
            .collect::<Result<Vec<_>, RepoError>>()?;

        Ok(CollabStats { items, daily })
    }

    async fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let res = sqlx::query("DELETE FROM ms_delivery_attempt WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(res.rows_affected() > 0)
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
        let mut a = repo.create("d1", "t1", ExecutorKind::ClaudeCode, None).await.expect("create");
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

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_list_tasks_carries_basic_info() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        for t in ["ms_delivery_attempt", "ms_task", "ms_task_decomposition", "ms_requirement"] {
            sqlx::query(&format!("TRUNCATE {t} CASCADE")).execute(&pool).await.expect("truncate");
        }

        sqlx::query("INSERT INTO ms_requirement (id, project_id, title, status) VALUES ('r1','p1','登录模块','DRAFT')")
            .execute(&pool).await.expect("req");
        sqlx::query("INSERT INTO ms_task_decomposition (id, requirement_id, requirement_version) VALUES ('d1','r1',1)")
            .execute(&pool).await.expect("decomp");
        sqlx::query("INSERT INTO ms_task (decomposition_id, id, title, description, status) VALUES ('d1','t1','实现登录接口','支持手机号+验证码登录','RUNNING')")
            .execute(&pool).await.expect("task");

        let repo = PgDeliveryRepository::new(pool.clone());
        repo.create("d1", "t1", ExecutorKind::ClaudeCode, None).await.expect("create");
        repo.create("d9", "t9", ExecutorKind::Codex, None).await.expect("create2");

        let page = repo
            .list_tasks(&TaskListFilter { limit: 10, ..Default::default() })
            .await
            .expect("list_tasks");
        assert_eq!(page.total, 2);
        let linked = page.items.iter().find(|r| r.attempt.task_id == "t1").expect("t1 row");
        assert_eq!(linked.title.as_deref(), Some("实现登录接口"));
        assert_eq!(linked.description.as_deref(), Some("支持手机号+验证码登录"));
        assert_eq!(linked.module.as_deref(), Some("登录模块"));
        let orphan = page.items.iter().find(|r| r.attempt.task_id == "t9").expect("t9 row");
        assert_eq!(orphan.title, None);
        assert_eq!(orphan.description, None);
        assert_eq!(orphan.module, None);
    }
}

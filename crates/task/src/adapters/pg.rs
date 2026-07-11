use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{Decomposition, Task, TaskStatus};
use crate::ports::{RepoError, TaskRepository};

#[derive(Clone)]
pub struct PgTaskRepository {
    pool: PgPool,
}

impl PgTaskRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn assemble(&self, meta: &sqlx::postgres::PgRow) -> Result<Decomposition, RepoError> {
        let id: String = meta.try_get("id").map_err(map_err)?;
        let version_i: i32 = meta.try_get("requirement_version").map_err(map_err)?;

        let drows = sqlx::query(
            "SELECT task_id, depends_on FROM ms_task_dependency WHERE decomposition_id = $1",
        )
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();
        for r in &drows {
            let task_id: String = r.try_get("task_id").map_err(map_err)?;
            let depends_on: String = r.try_get("depends_on").map_err(map_err)?;
            deps.entry(task_id).or_default().push(depends_on);
        }

        let trows = sqlx::query(
            "SELECT id, title, description, acceptance_criteria, status, points, assignee, assignee_kind FROM ms_task \
             WHERE decomposition_id = $1 ORDER BY seq",
        )
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        let mut tasks = Vec::with_capacity(trows.len());
        for t in &trows {
            let tid: String = t.try_get("id").map_err(map_err)?;
            let status_s: String = t.try_get("status").map_err(map_err)?;
            let criteria: Vec<String> = t.try_get("acceptance_criteria").map_err(map_err)?;
            tasks.push(Task {
                dependencies: deps.remove(&tid).unwrap_or_default(),
                id: tid,
                title: t.try_get("title").map_err(map_err)?,
                description: t.try_get("description").map_err(map_err)?,
                acceptance_criteria: criteria,
                status: TaskStatus::parse(&status_s).unwrap_or(TaskStatus::Pending),
                points: t.try_get("points").unwrap_or(0),
                assignee: t.try_get("assignee").unwrap_or_default(),
                assignee_kind: t.try_get("assignee_kind").unwrap_or_default(),
            });
        }

        Ok(Decomposition {
            id,
            requirement_id: meta.try_get("requirement_id").map_err(map_err)?,
            requirement_version: version_i as u32,
            tasks,
        })
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

const META_COLS: &str = "id, requirement_id, requirement_version";

#[async_trait]
impl TaskRepository for PgTaskRepository {
    async fn create_decomposition(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Decomposition, RepoError> {
        let id: String = sqlx::query(
            "INSERT INTO ms_task_decomposition (requirement_id, requirement_version) \
             VALUES ($1, $2) RETURNING id",
        )
        .bind(requirement_id)
        .bind(requirement_version as i32)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?
        .try_get("id")
        .map_err(map_err)?;
        Ok(Decomposition::new(&id, requirement_id, requirement_version))
    }

    async fn find_by_requirement_version(
        &self,
        requirement_id: &str,
        requirement_version: u32,
    ) -> Result<Option<Decomposition>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {META_COLS} FROM ms_task_decomposition \
             WHERE requirement_id = $1 AND requirement_version = $2"
        ))
        .bind(requirement_id)
        .bind(requirement_version as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(self.assemble(&r).await?)),
            None => Ok(None),
        }
    }

    async fn get(&self, id: &str) -> Result<Option<Decomposition>, RepoError> {
        let row =
            sqlx::query(&format!("SELECT {META_COLS} FROM ms_task_decomposition WHERE id = $1"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(self.assemble(&r).await?)),
            None => Ok(None),
        }
    }

    async fn save(&self, decomposition: &Decomposition) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        for t in &decomposition.tasks {
            sqlx::query(
                "INSERT INTO ms_task (decomposition_id, id, title, description, acceptance_criteria, status, points, assignee, assignee_kind) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
                 ON CONFLICT (decomposition_id, id) DO UPDATE \
                 SET title = EXCLUDED.title, description = EXCLUDED.description, \
                     acceptance_criteria = EXCLUDED.acceptance_criteria, status = EXCLUDED.status, \
                     points = EXCLUDED.points, assignee = EXCLUDED.assignee, assignee_kind = EXCLUDED.assignee_kind",
            )
            .bind(&decomposition.id)
            .bind(&t.id)
            .bind(&t.title)
            .bind(&t.description)
            .bind(&t.acceptance_criteria)
            .bind(t.status.as_str())
            .bind(t.points)
            .bind(&t.assignee)
            .bind(&t.assignee_kind)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;

            for dep in &t.dependencies {
                sqlx::query(
                    "INSERT INTO ms_task_dependency (decomposition_id, task_id, depends_on) \
                     VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
                )
                .bind(&decomposition.id)
                .bind(&t.id)
                .bind(dep)
                .execute(&mut *tx)
                .await
                .map_err(map_err)?;
            }
        }
        tx.commit().await.map_err(map_err)?;
        Ok(())
    }

    async fn save_task_status(
        &self,
        decomposition_id: &str,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<(), RepoError> {
        // Single-row update so concurrent sibling advances don't lose updates.
        sqlx::query("UPDATE ms_task SET status = $3 WHERE decomposition_id = $1 AND id = $2")
            .bind(decomposition_id)
            .bind(task_id)
            .bind(status.as_str())
            .execute(&self.pool)
            .await
            .map_err(map_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NewTask;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_dag_persistence_and_readiness() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_task_decomposition, ms_task, ms_task_dependency")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgTaskRepository::new(pool.clone());
        let mut d = repo.create_decomposition("req1", 1).await.expect("create");
        d.add_task(NewTask::new("A", "", &["验收A".into()], &[]).expect("v")).expect("a");
        d.add_task(NewTask::new("B", "", &[], &["t1".into()]).expect("v")).expect("b");
        repo.save(&d).await.expect("save");

        let mut got = repo.get(&d.id).await.expect("get").expect("some");
        assert_eq!(got.tasks.len(), 2);
        assert_eq!(got.task("t2").expect("t2").dependencies, vec!["t1".to_string()]);
        assert_eq!(got.ready_tasks().iter().map(|t| t.id.clone()).collect::<Vec<_>>(), vec!["t1"]);

        got.dispatch("t1").expect("dispatch");
        got.transition("t1", TaskStatus::Running).expect("run");
        got.transition("t1", TaskStatus::Delivered).expect("deliver");
        got.transition("t1", TaskStatus::Verified).expect("verify");
        repo.save(&got).await.expect("save");

        let reloaded = repo.get(&d.id).await.expect("get").expect("some");
        assert_eq!(reloaded.task("t1").expect("t1").status, TaskStatus::Verified);
        assert_eq!(
            reloaded.ready_tasks().iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
            vec!["t2"]
        );

        assert!(repo.find_by_requirement_version("req1", 1).await.expect("f").is_some());
    }
}

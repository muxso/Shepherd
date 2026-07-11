//! 集成测试 `#[ignore]`,需 DATABASE_URL:
//!   `DATABASE_URL=postgres://... cargo test -p requirement --features pg -- --ignored`

use async_trait::async_trait;
use sqlx::{PgPool, Row};

use crate::domain::{
    fill_stages, AcceptanceCriterion, ChangeEntry, NewChange, NewRequirement, Requirement,
    RequirementPriority, RequirementStatus, RequirementType, RequirementVersion, Stage, StageRow,
    StageStatus, StatusCounts, WorkStatus,
};
use crate::ports::{RepoError, RequirementRepository};

#[derive(Clone)]
pub struct PgRequirementRepository {
    pool: PgPool,
}

impl PgRequirementRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn assemble(&self, meta: &sqlx::postgres::PgRow) -> Result<Requirement, RepoError> {
        let id: String = meta.try_get("id").map_err(map_err)?;
        let baseline_i: i32 = meta.try_get("baseline_version").map_err(map_err)?;
        let status_s: String = meta.try_get("status").map_err(map_err)?;
        let priority_s: String = meta.try_get("priority").map_err(map_err)?;
        let req_type_s: String = meta.try_get("req_type").map_err(map_err)?;
        let dev_status_s: String = meta.try_get("dev_status").map_err(map_err)?;
        let test_status_s: String = meta.try_get("test_status").map_err(map_err)?;

        let vrows = sqlx::query(
            "SELECT version, description, acceptance_criteria FROM ms_requirement_version \
             WHERE requirement_id = $1 ORDER BY version",
        )
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        let mut versions = Vec::with_capacity(vrows.len());
        for v in &vrows {
            let version: i32 = v.try_get("version").map_err(map_err)?;
            let description: String = v.try_get("description").map_err(map_err)?;
            let criteria: Vec<String> = v.try_get("acceptance_criteria").map_err(map_err)?;
            versions.push(RequirementVersion {
                version: version as u32,
                description,
                acceptance_criteria: criteria
                    .into_iter()
                    .map(|text| AcceptanceCriterion { text })
                    .collect(),
            });
        }

        let stages = self.load_stages(&id).await?;

        Ok(Requirement {
            id,
            project_id: meta.try_get("project_id").map_err(map_err)?,
            title: meta.try_get("title").map_err(map_err)?,
            status: RequirementStatus::parse(&status_s).unwrap_or(RequirementStatus::Draft),
            baseline_version: baseline_i as u32,
            versions,
            deleted: meta.try_get("deleted").map_err(map_err)?,
            review_comment: meta.try_get("review_comment").map_err(map_err)?,
            priority: RequirementPriority::parse(&priority_s).unwrap_or_default(),
            req_type: RequirementType::parse(&req_type_s).unwrap_or_default(),
            tags: meta.try_get("tags").map_err(map_err)?,
            parent_id: meta.try_get("parent_id").map_err(map_err)?,
            due_date: meta.try_get("due_date").map_err(map_err)?,
            created_at_ms: meta.try_get("created_at_ms").map_err(map_err)?,
            updated_at_ms: meta.try_get("updated_at_ms").map_err(map_err)?,
            dev_status: WorkStatus::parse(&dev_status_s).unwrap_or_default(),
            test_status: WorkStatus::parse(&test_status_s).unwrap_or_default(),
            dev_started_at_ms: meta.try_get("dev_started_at_ms").map_err(map_err)?,
            dev_finished_at_ms: meta.try_get("dev_finished_at_ms").map_err(map_err)?,
            test_started_at_ms: meta.try_get("test_started_at_ms").map_err(map_err)?,
            test_finished_at_ms: meta.try_get("test_finished_at_ms").map_err(map_err)?,
            stages,
        })
    }

    /// 读阶段表并补齐成固定 7 行(未回填的旧数据得到 PENDING 默认行)。
    async fn load_stages(&self, requirement_id: &str) -> Result<Vec<StageRow>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {STAGE_COLS} FROM ms_requirement_stage WHERE requirement_id = $1"
        ))
        .bind(requirement_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let stage_s: String = r.try_get("stage").map_err(map_err)?;
            // 未知阶段(理论不该出现)忽略,不污染流水线。
            let Some(stage) = Stage::parse(&stage_s) else { continue };
            let status_s: String = r.try_get("status").map_err(map_err)?;
            out.push(StageRow {
                stage,
                status: StageStatus::parse(&status_s).unwrap_or_default(),
                planned_start: r.try_get("planned_start").map_err(map_err)?,
                planned_end: r.try_get("planned_end").map_err(map_err)?,
                started_at_ms: r.try_get("started_at_ms").map_err(map_err)?,
                finished_at_ms: r.try_get("finished_at_ms").map_err(map_err)?,
            });
        }
        Ok(fill_stages(&out))
    }
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

fn criteria_to_vec(v: &RequirementVersion) -> Vec<String> {
    v.acceptance_criteria.iter().map(|c| c.text.clone()).collect()
}

// 时间列统一转 epoch 毫秒(与全库口径一致);due_date 以 `YYYY-MM-DD` 文本进出。
const META_COLS: &str = "id, project_id, title, status, baseline_version, deleted, review_comment, priority, req_type, \
     tags, parent_id, due_date::text AS due_date, \
     (extract(epoch from created_at) * 1000)::bigint AS created_at_ms, \
     (extract(epoch from updated_at) * 1000)::bigint AS updated_at_ms, \
     dev_status, test_status, \
     (extract(epoch from dev_started_at) * 1000)::bigint AS dev_started_at_ms, \
     (extract(epoch from dev_finished_at) * 1000)::bigint AS dev_finished_at_ms, \
     (extract(epoch from test_started_at) * 1000)::bigint AS test_started_at_ms, \
     (extract(epoch from test_finished_at) * 1000)::bigint AS test_finished_at_ms";

// 阶段行:计划日期以 `YYYY-MM-DD` 文本进出,实际起止转 epoch 毫秒。
const STAGE_COLS: &str =
    "stage, status, planned_start::text AS planned_start, planned_end::text AS planned_end, \
     (extract(epoch from started_at) * 1000)::bigint AS started_at_ms, \
     (extract(epoch from finished_at) * 1000)::bigint AS finished_at_ms";

#[async_trait]
impl RequirementRepository for PgRequirementRepository {
    async fn find_active_by_title(
        &self,
        project_id: &str,
        title: &str,
    ) -> Result<Option<Requirement>, RepoError> {
        let row = sqlx::query(&format!(
            "SELECT {META_COLS} FROM ms_requirement \
             WHERE project_id = $1 AND title = $2 AND deleted = false"
        ))
        .bind(project_id)
        .bind(title)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(self.assemble(&r).await?)),
            None => Ok(None),
        }
    }

    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        let row = sqlx::query(
            "INSERT INTO ms_requirement (project_id, title, priority, req_type, tags, parent_id, due_date) \
             VALUES ($1, $2, $3, $4, $5, $6, $7::date) \
             RETURNING id, (extract(epoch from created_at) * 1000)::bigint AS created_at_ms, \
                       (extract(epoch from updated_at) * 1000)::bigint AS updated_at_ms",
        )
        .bind(&new.project_id)
        .bind(&new.title)
        .bind(new.priority.as_str())
        .bind(new.req_type.as_str())
        .bind(&new.tags)
        .bind(&new.parent_id)
        .bind(&new.due_date)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?;
        let id: String = row.try_get("id").map_err(map_err)?;

        let mut req = Requirement::create(&id, new);
        req.created_at_ms = row.try_get("created_at_ms").map_err(map_err)?;
        req.updated_at_ms = row.try_get("updated_at_ms").map_err(map_err)?;
        let v1 = req.latest();
        sqlx::query(
            "INSERT INTO ms_requirement_version (requirement_id, version, description, acceptance_criteria) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(&id)
        .bind(v1.version as i32)
        .bind(&v1.description)
        .bind(criteria_to_vec(v1))
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        tx.commit().await.map_err(map_err)?;
        Ok(req)
    }

    async fn get(&self, id: &str) -> Result<Option<Requirement>, RepoError> {
        let row = sqlx::query(&format!("SELECT {META_COLS} FROM ms_requirement WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_err)?;
        match row {
            Some(r) => Ok(Some(self.assemble(&r).await?)),
            None => Ok(None),
        }
    }

    async fn count_active(&self, project_id: &str) -> Result<u64, RepoError> {
        let row = sqlx::query(
            "SELECT count(*) AS n FROM ms_requirement WHERE project_id = $1 AND deleted = false",
        )
        .bind(project_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_err)?;
        let n: i64 = row.try_get("n").map_err(map_err)?;
        Ok(n as u64)
    }

    async fn list_active(
        &self,
        project_id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<Requirement>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {META_COLS} FROM ms_requirement \
             WHERE project_id = $1 AND deleted = false ORDER BY sort_order, seq LIMIT $2 OFFSET $3"
        ))
        .bind(project_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            out.push(self.assemble(r).await?);
        }
        Ok(out)
    }

    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        // 每次 save 盖更新时间;开发/测试起止毫秒转 timestamptz 落库。
        sqlx::query(
            "UPDATE ms_requirement SET title = $2, status = $3, baseline_version = $4, deleted = $5, review_comment = $6, \
             priority = $7, req_type = $8, tags = $9, parent_id = $10, due_date = $11::date, \
             dev_status = $12, test_status = $13, \
             dev_started_at = to_timestamp($14::double precision / 1000.0), \
             dev_finished_at = to_timestamp($15::double precision / 1000.0), \
             test_started_at = to_timestamp($16::double precision / 1000.0), \
             test_finished_at = to_timestamp($17::double precision / 1000.0), \
             updated_at = now() WHERE id = $1",
        )
        .bind(&requirement.id)
        .bind(&requirement.title)
        .bind(requirement.status.as_str())
        .bind(requirement.baseline_version as i32)
        .bind(requirement.deleted)
        .bind(&requirement.review_comment)
        .bind(requirement.priority.as_str())
        .bind(requirement.req_type.as_str())
        .bind(&requirement.tags)
        .bind(&requirement.parent_id)
        .bind(&requirement.due_date)
        .bind(requirement.dev_status.as_str())
        .bind(requirement.test_status.as_str())
        .bind(requirement.dev_started_at_ms)
        .bind(requirement.dev_finished_at_ms)
        .bind(requirement.test_started_at_ms)
        .bind(requirement.test_finished_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        // 版本不可变:ON CONFLICT DO NOTHING 只追加新版本,已存在的不改写。
        for v in &requirement.versions {
            sqlx::query(
                "INSERT INTO ms_requirement_version (requirement_id, version, description, acceptance_criteria) \
                 VALUES ($1, $2, $3, $4) ON CONFLICT (requirement_id, version) DO NOTHING",
            )
            .bind(&requirement.id)
            .bind(v.version as i32)
            .bind(&v.description)
            .bind(criteria_to_vec(v))
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }

        tx.commit().await.map_err(map_err)?;
        Ok(())
    }

    async fn set_order(&self, project_id: &str, ordered_ids: &[String]) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        for (i, id) in ordered_ids.iter().enumerate() {
            sqlx::query(
                "UPDATE ms_requirement SET sort_order = $1 WHERE id = $2 AND project_id = $3",
            )
            .bind(i as i64 + 1)
            .bind(id)
            .bind(project_id)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(())
    }

    async fn status_counts(&self, project_id: &str) -> Result<StatusCounts, RepoError> {
        let rows = sqlx::query(
            "SELECT status, count(*) AS n FROM ms_requirement \
             WHERE project_id = $1 AND deleted = false GROUP BY status",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut counts = StatusCounts::default();
        for row in &rows {
            let status: String = row.try_get("status").map_err(map_err)?;
            let n = row.try_get::<i64, _>("n").map_err(map_err)?.max(0) as u64;
            // 未知状态(理论不该出现)忽略,不污染计数。
            match RequirementStatus::parse(&status) {
                Some(RequirementStatus::Draft) => counts.draft += n,
                Some(RequirementStatus::Baselined) => counts.baselined += n,
                Some(RequirementStatus::Delivered) => counts.delivered += n,
                Some(RequirementStatus::Archived) => counts.archived += n,
                None => {}
            }
        }
        Ok(counts)
    }

    async fn children(&self, parent_id: &str) -> Result<Vec<Requirement>, RepoError> {
        let rows = sqlx::query(&format!(
            "SELECT {META_COLS} FROM ms_requirement \
             WHERE parent_id = $1 AND deleted = false ORDER BY sort_order, seq"
        ))
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            out.push(self.assemble(r).await?);
        }
        Ok(out)
    }

    async fn upsert_stage(&self, requirement_id: &str, row: &StageRow) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO ms_requirement_stage \
             (requirement_id, stage, status, planned_start, planned_end, started_at, finished_at) \
             VALUES ($1, $2, $3, $4::date, $5::date, \
                     to_timestamp($6::double precision / 1000.0), \
                     to_timestamp($7::double precision / 1000.0)) \
             ON CONFLICT (requirement_id, stage) DO UPDATE SET \
             status = EXCLUDED.status, planned_start = EXCLUDED.planned_start, \
             planned_end = EXCLUDED.planned_end, started_at = EXCLUDED.started_at, \
             finished_at = EXCLUDED.finished_at",
        )
        .bind(requirement_id)
        .bind(row.stage.as_str())
        .bind(row.status.as_str())
        .bind(&row.planned_start)
        .bind(&row.planned_end)
        .bind(row.started_at_ms)
        .bind(row.finished_at_ms)
        .execute(&self.pool)
        .await
        .map_err(map_err)?;
        Ok(())
    }

    async fn stages(&self, requirement_id: &str) -> Result<Vec<StageRow>, RepoError> {
        self.load_stages(requirement_id).await
    }

    async fn append_change(
        &self,
        requirement_id: &str,
        changes: &[NewChange],
    ) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        for c in changes {
            sqlx::query(
                "INSERT INTO ms_requirement_change (requirement_id, changed_by, field, old_value, new_value) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(requirement_id)
            .bind(&c.changed_by)
            .bind(&c.field)
            .bind(&c.old_value)
            .bind(&c.new_value)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        }
        tx.commit().await.map_err(map_err)?;
        Ok(())
    }

    async fn list_changes(
        &self,
        requirement_id: &str,
        limit: u32,
    ) -> Result<Vec<ChangeEntry>, RepoError> {
        let rows = sqlx::query(
            "SELECT (extract(epoch from changed_at) * 1000)::bigint AS changed_at_ms, \
                    changed_by, field, old_value, new_value \
             FROM ms_requirement_change WHERE requirement_id = $1 ORDER BY seq DESC LIMIT $2",
        )
        .bind(requirement_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            out.push(ChangeEntry {
                changed_at_ms: r.try_get("changed_at_ms").map_err(map_err)?,
                changed_by: r.try_get("changed_by").map_err(map_err)?,
                field: r.try_get("field").map_err(map_err)?,
                old_value: r.try_get("old_value").map_err(map_err)?,
                new_value: r.try_get("new_value").map_err(map_err)?,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_versioning_baseline_and_soft_delete() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_requirement, ms_requirement_version, ms_requirement_change, ms_requirement_stage")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgRequirementRepository::new(pool.clone());
        let nu = NewRequirement::new("p1", "登录", "用邮箱登录", &["正确凭证登录".to_string()])
            .expect("valid");

        let r = repo.insert(&nu).await.expect("insert");
        let mut got = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(got.latest_version(), 1);
        assert_eq!(got.baseline_version, 1);
        assert_eq!(got.baseline().acceptance_criteria[0].text, "正确凭证登录");
        // 新建默认优先级/类型落库。
        assert_eq!(got.priority, RequirementPriority::P2);
        assert_eq!(got.req_type, RequirementType::Feature);

        got.revise("v2 描述", vec![AcceptanceCriterion { text: "新增标准".into() }])
            .expect("revise");
        repo.save(&got).await.expect("save");
        let reloaded = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(reloaded.latest_version(), 2);
        assert_eq!(reloaded.baseline_version, 1);
        assert_eq!(reloaded.version(1).expect("v1").acceptance_criteria[0].text, "正确凭证登录");

        let mut r2 = reloaded;
        r2.set_baseline(2).expect("baseline");
        r2.priority = RequirementPriority::P0;
        r2.req_type = RequirementType::Bugfix;
        repo.save(&r2).await.expect("save");
        let after = repo.get(&r.id).await.expect("g").expect("s");
        assert_eq!(after.baseline_version, 2);
        assert_eq!(after.priority, RequirementPriority::P0);
        assert_eq!(after.req_type, RequirementType::Bugfix);

        assert!(repo.find_active_by_title("p1", "登录").await.expect("q").is_some());
        r2.soft_delete();
        repo.save(&r2).await.expect("save");
        assert!(repo.find_active_by_title("p1", "登录").await.expect("q").is_none());
        assert!(repo.insert(&nu).await.is_ok());
    }

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_lifecycle_fields_children_and_change_log_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_requirement, ms_requirement_version, ms_requirement_change, ms_requirement_stage")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgRequirementRepository::new(pool.clone());
        let parent = repo
            .insert(&NewRequirement::new("p1", "父需求", "d", &[]).expect("valid"))
            .await
            .expect("insert parent");
        let child_new = NewRequirement::new("p1", "子需求", "d", &[])
            .expect("valid")
            .with_tags(vec!["api".to_string(), "web".to_string()])
            .with_due_date(Some("2026-12-31".to_string()))
            .with_parent(Some(parent.id.clone()));
        let child = repo.insert(&child_new).await.expect("insert child");
        assert!(child.created_at_ms > 0);
        assert_eq!(child.created_at_ms, child.updated_at_ms);

        // 生命周期字段落库并可读回。
        let mut got = repo.get(&child.id).await.expect("get").expect("some");
        assert_eq!(got.tags, vec!["api".to_string(), "web".to_string()]);
        assert_eq!(got.due_date.as_deref(), Some("2026-12-31"));
        assert_eq!(got.parent_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(got.dev_status, WorkStatus::NotStarted);

        got.set_dev_status(WorkStatus::InProgress, 1_700_000_000_000);
        got.set_test_status(WorkStatus::Done, 1_700_000_100_000);
        got.due_date = None;
        repo.save(&got).await.expect("save");
        let reloaded = repo.get(&child.id).await.expect("get").expect("some");
        assert_eq!(reloaded.dev_status, WorkStatus::InProgress);
        assert_eq!(reloaded.dev_started_at_ms, Some(1_700_000_000_000));
        assert_eq!(reloaded.test_status, WorkStatus::Done);
        assert_eq!(reloaded.test_started_at_ms, Some(1_700_000_100_000));
        assert_eq!(reloaded.test_finished_at_ms, Some(1_700_000_100_000));
        assert!(reloaded.due_date.is_none());
        // save 盖了更新时间。
        assert!(reloaded.updated_at_ms >= reloaded.created_at_ms);

        // 子需求列表。
        let kids = repo.children(&parent.id).await.expect("children");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].id, child.id);

        // 变更日志:追加后倒序读回,limit 生效。
        let mk = |field: &str, old: &str, new: &str| NewChange {
            changed_by: "u1".to_string(),
            field: field.to_string(),
            old_value: old.to_string(),
            new_value: new.to_string(),
        };
        repo.append_change(
            &child.id,
            &[
                mk("devStatus", "NOT_STARTED", "IN_PROGRESS"),
                mk("testStatus", "NOT_STARTED", "DONE"),
            ],
        )
        .await
        .expect("append");
        let log = repo.list_changes(&child.id, 200).await.expect("changes");
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].field, "testStatus");
        assert_eq!(log[1].field, "devStatus");
        assert_eq!(log[0].changed_by, "u1");
        assert!(log[0].changed_at_ms > 0);
        assert_eq!(repo.list_changes(&child.id, 1).await.expect("changes").len(), 1);
    }

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_stage_upsert_roundtrip_and_backfill_from_old_shape_row() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_requirement, ms_requirement_version, ms_requirement_change, ms_requirement_stage")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgRequirementRepository::new(pool.clone());

        // —— upsert/读回 ——:未写行的需求读回 7 行 PENDING 默认。
        let r = repo
            .insert(&NewRequirement::new("p1", "阶段需求", "d", &[]).expect("valid"))
            .await
            .expect("insert");
        let rows = repo.stages(&r.id).await.expect("stages");
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().all(|s| s.status == StageStatus::Pending));

        let mut dev = StageRow::pending(Stage::Dev);
        dev.planned_start = Some("2026-07-01".to_string());
        dev.planned_end = Some("2026-07-31".to_string());
        dev.set_status(StageStatus::InProgress, 1_700_000_000_000);
        repo.upsert_stage(&r.id, &dev).await.expect("upsert");
        // 幂等覆盖同一行。
        dev.set_status(StageStatus::Done, 1_700_000_100_000);
        repo.upsert_stage(&r.id, &dev).await.expect("upsert");

        let rows = repo.stages(&r.id).await.expect("stages");
        assert_eq!(rows[3], dev);
        let got = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(got.stages, rows);

        // —— 回填 ——:直接 SQL 造一条「旧形态」需求(只有 dev_/test_ 列,无阶段行),
        // 重放迁移文件(幂等)后应得到按旧列推导的 7 行。
        let old_row = sqlx::query(
            "INSERT INTO ms_requirement \
             (project_id, title, status, dev_status, test_status, dev_started_at, dev_finished_at, \
              test_started_at, test_finished_at) \
             VALUES ('p1', '旧需求', 'BASELINED', 'IN_PROGRESS', 'DONE', \
                     to_timestamp(1700000000), NULL, to_timestamp(1700000100), to_timestamp(1700000200)) \
             RETURNING id, (extract(epoch from created_at) * 1000)::bigint AS created_at_ms",
        )
        .fetch_one(&pool)
        .await
        .expect("seed old-shape row");
        let old_id: String = old_row.try_get("id").expect("id");
        let old_created_ms: i64 = old_row.try_get("created_at_ms").expect("created_at_ms");

        sqlx::raw_sql(include_str!("../../../migrate/migrations/0084_requirement_stage.sql"))
            .execute(&pool)
            .await
            .expect("replay backfill");

        let stages = repo.stages(&old_id).await.expect("stages");
        assert_eq!(stages.iter().map(|s| s.stage).collect::<Vec<_>>(), Stage::ALL.to_vec());
        // CREATED:完成,起止 = created_at。
        assert_eq!(stages[0].status, StageStatus::Done);
        assert_eq!(stages[0].started_at_ms, Some(old_created_ms));
        assert_eq!(stages[0].finished_at_ms, Some(old_created_ms));
        // AUDIT:新阶段,待处理。
        assert_eq!(stages[1].status, StageStatus::Pending);
        // REVIEW:BASELINED → 视为评审通过。
        assert_eq!(stages[2].status, StageStatus::Done);
        // DEV:沿用旧状态与起止时间。
        assert_eq!(stages[3].status, StageStatus::InProgress);
        assert_eq!(stages[3].started_at_ms, Some(1_700_000_000_000));
        assert_eq!(stages[3].finished_at_ms, None);
        // TEST:同上。
        assert_eq!(stages[4].status, StageStatus::Done);
        assert_eq!(stages[4].started_at_ms, Some(1_700_000_100_000));
        assert_eq!(stages[4].finished_at_ms, Some(1_700_000_200_000));
        // ACCEPTANCE/DELIVERY:未交付 → 待处理。
        assert_eq!(stages[5].status, StageStatus::Pending);
        assert_eq!(stages[6].status, StageStatus::Pending);

        // 回填幂等:已有行(含前面 upsert 的 DEV 行)不被改写。
        sqlx::raw_sql(include_str!("../../../migrate/migrations/0084_requirement_stage.sql"))
            .execute(&pool)
            .await
            .expect("replay again");
        assert_eq!(repo.stages(&r.id).await.expect("stages")[3], dev);
    }
}

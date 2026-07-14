//! Integration tests are `#[ignore]`d and need DATABASE_URL:
//!   `DATABASE_URL=postgres://... cargo test -p requirement --features pg -- --ignored`

use std::collections::HashMap;

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

    /// Assemble one requirement; single-row reads go through the batch path.
    async fn assemble_one(&self, row: sqlx::postgres::PgRow) -> Result<Requirement, RepoError> {
        self.assemble_many(vec![row])
            .await?
            .pop()
            .ok_or_else(|| RepoError::Backend("assemble_many returned no row".to_string()))
    }

    /// Batch-assemble requirements: two `= ANY($1)` queries fetch all versions and
    /// stages for the page, grouped in memory. Output preserves input row order.
    async fn assemble_many(
        &self,
        rows: Vec<sqlx::postgres::PgRow>,
    ) -> Result<Vec<Requirement>, RepoError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let mut reqs = Vec::with_capacity(rows.len());
        for r in &rows {
            reqs.push(parse_meta(r)?);
        }
        let ids: Vec<String> = reqs.iter().map(|r| r.id.clone()).collect();

        let vrows = sqlx::query(
            "SELECT requirement_id, version, description, acceptance_criteria \
             FROM ms_requirement_version \
             WHERE requirement_id = ANY($1) ORDER BY requirement_id, version",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut versions: HashMap<String, Vec<RequirementVersion>> = HashMap::new();
        for v in &vrows {
            let rid: String = v.try_get("requirement_id").map_err(map_err)?;
            versions.entry(rid).or_default().push(parse_version(v)?);
        }

        let srows = sqlx::query(&format!(
            "SELECT requirement_id, {STAGE_COLS} FROM ms_requirement_stage \
             WHERE requirement_id = ANY($1) ORDER BY requirement_id, stage"
        ))
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_err)?;
        let mut stages: HashMap<String, Vec<StageRow>> = HashMap::new();
        for s in &srows {
            let rid: String = s.try_get("requirement_id").map_err(map_err)?;
            if let Some(row) = parse_stage(s)? {
                stages.entry(rid).or_default().push(row);
            }
        }

        for req in &mut reqs {
            req.versions = versions.remove(&req.id).unwrap_or_default();
            req.stages = fill_stages(&stages.remove(&req.id).unwrap_or_default());
        }
        Ok(reqs)
    }

    /// Load stage rows and fill up to the fixed 7 (non-backfilled legacy data gets PENDING defaults).
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
            if let Some(row) = parse_stage(r)? {
                out.push(row);
            }
        }
        Ok(fill_stages(&out))
    }
}

/// Parse the ms_requirement columns; versions/stages are filled in by the caller.
fn parse_meta(meta: &sqlx::postgres::PgRow) -> Result<Requirement, RepoError> {
    let baseline_i: i32 = meta.try_get("baseline_version").map_err(map_err)?;
    let status_s: String = meta.try_get("status").map_err(map_err)?;
    let priority_s: String = meta.try_get("priority").map_err(map_err)?;
    let req_type_s: String = meta.try_get("req_type").map_err(map_err)?;
    let dev_status_s: String = meta.try_get("dev_status").map_err(map_err)?;
    let test_status_s: String = meta.try_get("test_status").map_err(map_err)?;
    let custom: serde_json::Value = meta.try_get("custom_fields").map_err(map_err)?;

    Ok(Requirement {
        id: meta.try_get("id").map_err(map_err)?,
        project_id: meta.try_get("project_id").map_err(map_err)?,
        title: meta.try_get("title").map_err(map_err)?,
        status: RequirementStatus::parse(&status_s).unwrap_or(RequirementStatus::Draft),
        baseline_version: baseline_i as u32,
        versions: Vec::new(),
        deleted: meta.try_get("deleted").map_err(map_err)?,
        review_comment: meta.try_get("review_comment").map_err(map_err)?,
        priority: RequirementPriority::parse(&priority_s).unwrap_or_default(),
        req_type: RequirementType::parse(&req_type_s).unwrap_or_default(),
        tags: meta.try_get("tags").map_err(map_err)?,
        parent_id: meta.try_get("parent_id").map_err(map_err)?,
        custom_fields: json_to_fields(&custom),
        module_id: meta.try_get("module_id").map_err(map_err)?,
        skill_ids: meta.try_get("skill_ids").map_err(map_err)?,
        due_date: meta.try_get("due_date").map_err(map_err)?,
        created_at_ms: meta.try_get("created_at_ms").map_err(map_err)?,
        updated_at_ms: meta.try_get("updated_at_ms").map_err(map_err)?,
        created_by: meta
            .try_get::<Option<String>, _>("created_by")
            .map_err(map_err)?
            .unwrap_or_default(),
        dev_status: WorkStatus::parse(&dev_status_s).unwrap_or_default(),
        test_status: WorkStatus::parse(&test_status_s).unwrap_or_default(),
        dev_started_at_ms: meta.try_get("dev_started_at_ms").map_err(map_err)?,
        dev_finished_at_ms: meta.try_get("dev_finished_at_ms").map_err(map_err)?,
        test_started_at_ms: meta.try_get("test_started_at_ms").map_err(map_err)?,
        test_finished_at_ms: meta.try_get("test_finished_at_ms").map_err(map_err)?,
        stages: Vec::new(),
    })
}

fn parse_version(v: &sqlx::postgres::PgRow) -> Result<RequirementVersion, RepoError> {
    let version: i32 = v.try_get("version").map_err(map_err)?;
    let description: String = v.try_get("description").map_err(map_err)?;
    let criteria: Vec<String> = v.try_get("acceptance_criteria").map_err(map_err)?;
    Ok(RequirementVersion {
        version: version as u32,
        description,
        acceptance_criteria: criteria
            .into_iter()
            .map(|text| AcceptanceCriterion { text })
            .collect(),
    })
}

/// Parse one ms_requirement_stage row; unknown stages (should not happen) are dropped.
fn parse_stage(r: &sqlx::postgres::PgRow) -> Result<Option<StageRow>, RepoError> {
    let stage_s: String = r.try_get("stage").map_err(map_err)?;
    let Some(stage) = Stage::parse(&stage_s) else { return Ok(None) };
    let status_s: String = r.try_get("status").map_err(map_err)?;
    Ok(Some(StageRow {
        stage,
        status: StageStatus::parse(&status_s).unwrap_or_default(),
        planned_start: r.try_get("planned_start").map_err(map_err)?,
        planned_end: r.try_get("planned_end").map_err(map_err)?,
        started_at_ms: r.try_get("started_at_ms").map_err(map_err)?,
        finished_at_ms: r.try_get("finished_at_ms").map_err(map_err)?,
    }))
}

fn map_err(e: sqlx::Error) -> RepoError {
    RepoError::Backend(e.to_string())
}

/// Custom field map → JSONB object (values are always strings).
fn fields_to_json(f: &std::collections::BTreeMap<String, String>) -> serde_json::Value {
    serde_json::Value::Object(
        f.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect(),
    )
}

/// JSONB object → custom field map; non-string values (should not happen) fall back to their JSON text.
fn json_to_fields(v: &serde_json::Value) -> std::collections::BTreeMap<String, String> {
    v.as_object()
        .map(|obj| {
            obj.iter()
                .map(|(k, val)| {
                    let s = match val {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), s)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn criteria_to_vec(v: &RequirementVersion) -> Vec<String> {
    v.acceptance_criteria.iter().map(|c| c.text.clone()).collect()
}

// Time columns convert to epoch millis (consistent DB-wide); due_date round-trips as `YYYY-MM-DD` text.
const META_COLS: &str = "id, project_id, title, status, baseline_version, deleted, review_comment, priority, req_type, \
     tags, parent_id, custom_fields, module_id, skill_ids, created_by, due_date::text AS due_date, \
     (extract(epoch from created_at) * 1000)::bigint AS created_at_ms, \
     (extract(epoch from updated_at) * 1000)::bigint AS updated_at_ms, \
     dev_status, test_status, \
     (extract(epoch from dev_started_at) * 1000)::bigint AS dev_started_at_ms, \
     (extract(epoch from dev_finished_at) * 1000)::bigint AS dev_finished_at_ms, \
     (extract(epoch from test_started_at) * 1000)::bigint AS test_started_at_ms, \
     (extract(epoch from test_finished_at) * 1000)::bigint AS test_finished_at_ms";

// Stage rows: planned dates round-trip as `YYYY-MM-DD` text; actual start/finish convert to epoch millis.
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
            Some(r) => Ok(Some(self.assemble_one(r).await?)),
            None => Ok(None),
        }
    }

    async fn insert(&self, new: &NewRequirement) -> Result<Requirement, RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        let row = sqlx::query(
            "INSERT INTO ms_requirement (project_id, title, priority, req_type, tags, parent_id, due_date, custom_fields, module_id, skill_ids, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7::date, $8, $9, $10, $11) \
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
        .bind(fields_to_json(&new.custom_fields))
        .bind(&new.module_id)
        .bind(&new.skill_ids)
        .bind(&new.created_by)
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
            Some(r) => Ok(Some(self.assemble_one(r).await?)),
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

        self.assemble_many(rows).await
    }

    async fn save(&self, requirement: &Requirement) -> Result<(), RepoError> {
        let mut tx = self.pool.begin().await.map_err(map_err)?;
        // Every save stamps updated_at; dev/test start/finish millis are stored as timestamptz.
        sqlx::query(
            "UPDATE ms_requirement SET title = $2, status = $3, baseline_version = $4, deleted = $5, review_comment = $6, \
             priority = $7, req_type = $8, tags = $9, parent_id = $10, due_date = $11::date, \
             dev_status = $12, test_status = $13, \
             dev_started_at = to_timestamp($14::double precision / 1000.0), \
             dev_finished_at = to_timestamp($15::double precision / 1000.0), \
             test_started_at = to_timestamp($16::double precision / 1000.0), \
             test_finished_at = to_timestamp($17::double precision / 1000.0), \
             custom_fields = $18, module_id = $19, updated_at = now() WHERE id = $1",
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
        .bind(fields_to_json(&requirement.custom_fields))
        .bind(&requirement.module_id)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;

        // Versions are immutable: ON CONFLICT DO NOTHING only appends new versions, never rewrites.
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
            // Ignore unknown statuses (should not happen) instead of skewing counts.
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
        self.assemble_many(rows).await
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
        // Default priority/type are persisted on insert.
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
        let cf = |pairs: &[(&str, &str)]| -> std::collections::BTreeMap<String, String> {
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let child_new = NewRequirement::new("p1", "子需求", "d", &[])
            .expect("valid")
            .with_tags(vec!["api".to_string(), "web".to_string()])
            .with_due_date(Some("2026-12-31".to_string()))
            .with_parent(Some(parent.id.clone()))
            .with_custom_fields(cf(&[("owner", "alice"), ("多选", "a,b,c")]))
            .with_module("mod-1");
        let child = repo.insert(&child_new).await.expect("insert child");
        assert!(child.created_at_ms > 0);
        assert_eq!(child.created_at_ms, child.updated_at_ms);

        // Lifecycle fields persist and read back.
        let mut got = repo.get(&child.id).await.expect("get").expect("some");
        assert_eq!(got.tags, vec!["api".to_string(), "web".to_string()]);
        assert_eq!(got.due_date.as_deref(), Some("2026-12-31"));
        assert_eq!(got.parent_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(got.dev_status, WorkStatus::NotStarted);
        // Custom fields round-trip; the parent never wrote any, so it reads an empty map.
        assert_eq!(got.custom_fields, cf(&[("owner", "alice"), ("多选", "a,b,c")]));
        assert!(parent.custom_fields.is_empty());
        // Module id round-trips; no module defaults to empty string (unfiled).
        assert_eq!(got.module_id, "mod-1");
        assert_eq!(parent.module_id, "");

        got.set_dev_status(WorkStatus::InProgress, 1_700_000_000_000);
        got.set_test_status(WorkStatus::Done, 1_700_000_100_000);
        got.due_date = None;
        got.custom_fields = cf(&[("owner", "bob"), ("env", "prod")]);
        got.module_id = "mod-2".to_string();
        repo.save(&got).await.expect("save");
        let reloaded = repo.get(&child.id).await.expect("get").expect("some");
        // save replaces custom fields wholesale; module updates with save.
        assert_eq!(reloaded.custom_fields, cf(&[("owner", "bob"), ("env", "prod")]));
        assert_eq!(reloaded.module_id, "mod-2");
        assert_eq!(reloaded.dev_status, WorkStatus::InProgress);
        assert_eq!(reloaded.dev_started_at_ms, Some(1_700_000_000_000));
        assert_eq!(reloaded.test_status, WorkStatus::Done);
        assert_eq!(reloaded.test_started_at_ms, Some(1_700_000_100_000));
        assert_eq!(reloaded.test_finished_at_ms, Some(1_700_000_100_000));
        assert!(reloaded.due_date.is_none());
        // save stamped updated_at.
        assert!(reloaded.updated_at_ms >= reloaded.created_at_ms);

        let kids = repo.children(&parent.id).await.expect("children");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].id, child.id);

        // Change log: reads back newest-first after append; limit applies.
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

        // —— upsert/read-back ——: a requirement with no stage rows reads back 7 PENDING defaults.
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
        // Idempotent overwrite of the same row.
        dev.set_status(StageStatus::Done, 1_700_000_100_000);
        repo.upsert_stage(&r.id, &dev).await.expect("upsert");

        let rows = repo.stages(&r.id).await.expect("stages");
        assert_eq!(rows[3], dev);
        let got = repo.get(&r.id).await.expect("get").expect("some");
        assert_eq!(got.stages, rows);

        // —— backfill ——: seed an old-shape requirement via raw SQL (dev_/test_ columns only,
        // no stage rows); replaying the (idempotent) migration derives the 7 rows from the old columns.
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
        // CREATED: done, start/finish = created_at.
        assert_eq!(stages[0].status, StageStatus::Done);
        assert_eq!(stages[0].started_at_ms, Some(old_created_ms));
        assert_eq!(stages[0].finished_at_ms, Some(old_created_ms));
        // AUDIT: new stage, pending.
        assert_eq!(stages[1].status, StageStatus::Pending);
        // REVIEW: BASELINED → treated as review passed.
        assert_eq!(stages[2].status, StageStatus::Done);
        // DEV: carries over the old status and timestamps.
        assert_eq!(stages[3].status, StageStatus::InProgress);
        assert_eq!(stages[3].started_at_ms, Some(1_700_000_000_000));
        assert_eq!(stages[3].finished_at_ms, None);
        // TEST: same.
        assert_eq!(stages[4].status, StageStatus::Done);
        assert_eq!(stages[4].started_at_ms, Some(1_700_000_100_000));
        assert_eq!(stages[4].finished_at_ms, Some(1_700_000_200_000));
        // ACCEPTANCE/DELIVERY: not delivered → pending.
        assert_eq!(stages[5].status, StageStatus::Pending);
        assert_eq!(stages[6].status, StageStatus::Pending);

        // Backfill is idempotent: existing rows (incl. the DEV row upserted above) stay untouched.
        sqlx::raw_sql(include_str!("../../../migrate/migrations/0084_requirement_stage.sql"))
            .execute(&pool)
            .await
            .expect("replay again");
        assert_eq!(repo.stages(&r.id).await.expect("stages")[3], dev);
    }

    #[tokio::test]
    #[ignore = "需要 DATABASE_URL 指向一个 PostgreSQL 实例"]
    async fn pg_list_active_batch_assembly_groups_versions_and_stages() {
        let url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&url).await.expect("connect");
        migrate::run(&pool).await.expect("migrate");
        sqlx::query("TRUNCATE ms_requirement, ms_requirement_version, ms_requirement_change, ms_requirement_stage")
            .execute(&pool)
            .await
            .expect("truncate");

        let repo = PgRequirementRepository::new(pool.clone());

        // 3 requirements, each revised to 2 versions.
        let mut ids = Vec::new();
        for i in 1..=3 {
            let nu = NewRequirement::new(
                "p1",
                &format!("批量需求{i}"),
                &format!("r{i} v1"),
                &[format!("r{i} 标准1")],
            )
            .expect("valid");
            let r = repo.insert(&nu).await.expect("insert");
            let mut got = repo.get(&r.id).await.expect("get").expect("some");
            got.revise(
                &format!("r{i} v2"),
                vec![AcceptanceCriterion { text: format!("r{i} 标准2") }],
            )
            .expect("revise");
            repo.save(&got).await.expect("save");
            ids.push(r.id);
        }

        // Stage rows only on #1 (DEV) and #3 (TEST); #2 gets fill_stages defaults.
        let mut dev = StageRow::pending(Stage::Dev);
        dev.set_status(StageStatus::InProgress, 1_700_000_000_000);
        repo.upsert_stage(&ids[0], &dev).await.expect("upsert dev");
        let mut test = StageRow::pending(Stage::Test);
        test.set_status(StageStatus::Done, 1_700_000_200_000);
        repo.upsert_stage(&ids[2], &test).await.expect("upsert test");

        let page = repo.list_active("p1", 0, 50).await.expect("list");
        assert_eq!(page.len(), 3);
        // Stable order: insertion order (sort_order, seq).
        assert_eq!(
            page.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ids.iter().map(String::as_str).collect::<Vec<_>>()
        );

        // Versions land on their own requirement, ordered by version.
        for (i, r) in page.iter().enumerate() {
            let n = i + 1;
            assert_eq!(r.versions.len(), 2, "requirement {n} versions");
            assert_eq!(r.versions[0].version, 1);
            assert_eq!(r.versions[0].description, format!("r{n} v1"));
            assert_eq!(r.versions[0].acceptance_criteria[0].text, format!("r{n} 标准1"));
            assert_eq!(r.versions[1].version, 2);
            assert_eq!(r.versions[1].description, format!("r{n} v2"));
            assert_eq!(r.versions[1].acceptance_criteria[0].text, format!("r{n} 标准2"));
            assert_eq!(r.stages.len(), 7);
        }

        // Stage rows land on their own requirement; the rest stay PENDING.
        assert_eq!(page[0].stages[3], dev);
        assert!(page[0]
            .stages
            .iter()
            .enumerate()
            .all(|(j, s)| j == 3 || s.status == StageStatus::Pending));
        assert!(page[1].stages.iter().all(|s| s.status == StageStatus::Pending));
        assert_eq!(page[2].stages[4], test);
        assert!(page[2]
            .stages
            .iter()
            .enumerate()
            .all(|(j, s)| j == 4 || s.status == StageStatus::Pending));

        // Batch and single-row paths agree field for field.
        let single = repo.get(&ids[1]).await.expect("get").expect("some");
        assert_eq!(single, page[1]);
    }
}

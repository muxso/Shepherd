use crate::{
    domain::{CompleteRun, Error, ListQuery, NewSource, Record},
    ports::Store,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
#[derive(Clone)]
pub struct PgStore(pub PgPool);
pub(super) fn db(e: sqlx::Error) -> Error {
    tracing::error!(error=%e,"DevOps storage failed");
    Error::Storage
}
const LATE: &str = "kind='requirement' AND payload->>'priority' IN ('紧急','高','中') AND payload->>'status' NOT IN ('已发布','已取消','已完成','已关闭','已验收','完成') AND NULLIF(payload->>'due','')::date < (now() AT TIME ZONE 'Asia/Shanghai')::date";
#[async_trait]
impl Store for PgStore {
    async fn sources(&self) -> Result<Value, Error> {
        let rows=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(s) || jsonb_build_object('projectName',p.name) FROM ms_devops_source s JOIN ms_project p ON p.id=s.project_id ORDER BY s.name").fetch_all(&self.0).await.map_err(db)?;
        Ok(json!({"items":rows}))
    }
    async fn create_source(&self, s: NewSource) -> Result<Value, Error> {
        s.validate()?;
        let project = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM ms_project WHERE id=$1 AND NOT deleted)",
        )
        .bind(&s.project_id)
        .fetch_one(&self.0)
        .await
        .map_err(db)?;
        if !project {
            return Err(Error::NotFound);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let row=sqlx::query_scalar::<_,Value>("INSERT INTO ms_devops_source(id,project_id,cloud_org_id,cloud_project_id,name) VALUES($1,$2,$3,$4,$5) ON CONFLICT(cloud_org_id,cloud_project_id) DO UPDATE SET name=EXCLUDED.name WHERE ms_devops_source.project_id=EXCLUDED.project_id RETURNING to_jsonb(ms_devops_source)")
   .bind(id).bind(s.project_id).bind(s.cloud_org_id).bind(s.cloud_project_id).bind(s.name).fetch_optional(&self.0).await.map_err(db)?;
        row.ok_or(Error::Conflict(
            "cloud project already mapped to a different Shepherd project".into(),
        ))
    }
    async fn start(&self, source: &str) -> Result<Value, Error> {
        let mut tx = self.0.begin().await.map_err(db)?;
        let found = sqlx::query_scalar::<_, String>(
            "SELECT id FROM ms_devops_source WHERE id=$1 FOR UPDATE",
        )
        .bind(source)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if found.is_none() {
            return Err(Error::NotFound);
        }
        sqlx::query("UPDATE ms_devops_sync_run SET status='FAILED',finished_at=now(),error='sync expired after 1 hour' WHERE source_id=$1 AND status='RUNNING' AND started_at<now()-interval '1 hour'").bind(source).execute(&mut *tx).await.map_err(db)?;
        let running=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM ms_devops_sync_run WHERE source_id=$1 AND status='RUNNING')").bind(source).fetch_one(&mut *tx).await.map_err(db)?;
        if running {
            return Err(Error::Conflict("a sync is already running".into()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO ms_devops_sync_run(id,source_id,status) VALUES($1,$2,'RUNNING')")
            .bind(&id)
            .bind(source)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(json!({"id":id,"status":"RUNNING"}))
    }
    async fn stage(&self, run: &str, records: Vec<Record>) -> Result<Value, Error> {
        let mut tx = self.0.begin().await.map_err(db)?;
        lock_run(&mut tx, run).await?;
        for r in &records {
            sqlx::query("INSERT INTO ms_devops_stage(run_id,kind,external_id,payload) VALUES($1,$2,$3,$4) ON CONFLICT(run_id,kind,external_id) DO UPDATE SET payload=EXCLUDED.payload").bind(run).bind(&r.kind).bind(&r.external_id).bind(&r.data).execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(json!({"staged":records.len()}))
    }
    async fn complete(&self, run: &str, b: CompleteRun) -> Result<Value, Error> {
        b.validate()?;
        let mut tx = self.0.begin().await.map_err(db)?;
        let source = lock_run(&mut tx, run).await?;
        let counts = sqlx::query(
            "SELECT kind,count(*) AS n FROM ms_devops_stage WHERE run_id=$1 GROUP BY kind",
        )
        .bind(run)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let mut actual = std::collections::BTreeMap::<String, i64>::new();
        for kind in crate::domain::KINDS {
            actual.insert((*kind).into(), 0);
        }
        for r in counts {
            actual.insert(r.get("kind"), r.get("n"));
        }
        let mut expected = b.counts.clone();
        expected.entry("testcase".into()).or_insert(0);
        if actual != expected {
            return Err(Error::Conflict(
                "staged counts do not match complete snapshot; previous data retained".into(),
            ));
        }
        sqlx::query("INSERT INTO ms_devops_history(source_id,kind,external_id,old_status,new_status,source_changed_at) SELECT $2,s.kind,s.external_id,e.payload->>'status',s.payload->>'status',s.payload->>'statusChangedAt' FROM ms_devops_stage s JOIN ms_devops_entity e ON e.source_id=$2 AND e.kind=s.kind AND e.external_id=s.external_id WHERE s.run_id=$1 AND s.kind IN ('requirement','bug') AND e.payload->>'status' IS DISTINCT FROM s.payload->>'status'").bind(run).bind(&source).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO ms_devops_entity(source_id,kind,external_id,payload) SELECT $2,kind,external_id,payload FROM ms_devops_stage WHERE run_id=$1 ON CONFLICT(source_id,kind,external_id) DO UPDATE SET payload=EXCLUDED.payload,active=true,observed_at=now() WHERE ms_devops_entity.payload IS DISTINCT FROM EXCLUDED.payload OR NOT ms_devops_entity.active").bind(run).bind(&source).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE ms_devops_entity e SET active=false,observed_at=now() WHERE e.source_id=$2 AND e.kind IN ('requirement','bug','sprint') AND e.active AND NOT EXISTS(SELECT 1 FROM ms_devops_stage s WHERE s.run_id=$1 AND s.kind=e.kind AND s.external_id=e.external_id)").bind(run).bind(&source).execute(&mut *tx).await.map_err(db)?;
        if b.counts.contains_key("testcase") {
            sqlx::query("UPDATE ms_devops_entity e SET active=false,observed_at=now() WHERE e.source_id=$2 AND e.kind='testcase' AND e.active AND NOT EXISTS(SELECT 1 FROM ms_devops_stage s WHERE s.run_id=$1 AND s.kind='testcase' AND s.external_id=e.external_id)").bind(run).bind(&source).execute(&mut *tx).await.map_err(db)?;
        }
        super::mirror::project(&mut tx, &source).await?;
        // Refresh the successfully scanned code window; older facts remain available for history.
        sqlx::query("UPDATE ms_devops_entity e SET active=false,observed_at=now() WHERE e.source_id=$2 AND e.kind='commit' AND e.active AND (payload->>'authoredAt')::timestamptz >= $3::text::timestamptz AND (payload->>'authoredAt')::timestamptz < $4::text::timestamptz AND NOT EXISTS(SELECT 1 FROM ms_devops_stage s WHERE s.run_id=$1 AND s.kind='commit' AND s.external_id=e.external_id)").bind(run).bind(&source).bind(&b.code_since).bind(&b.code_until).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE ms_devops_source SET last_success_at=now(),code_since=LEAST(COALESCE(code_since,$2::text::timestamptz),$2::text::timestamptz),code_until=$3::text::timestamptz WHERE id=$1").bind(&source).bind(&b.code_since).bind(&b.code_until).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE ms_devops_sync_run SET status='SUCCESS',finished_at=now(),counts=$2 WHERE id=$1").bind(run).bind(json!(actual)).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM ms_devops_stage WHERE run_id=$1")
            .bind(run)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(json!({"id":run,"status":"SUCCESS","counts":actual}))
    }
    async fn fail(&self, run: &str, error: &str) -> Result<Value, Error> {
        let mut tx = self.0.begin().await.map_err(db)?;
        lock_run(&mut tx, run).await?;
        sqlx::query(
            "UPDATE ms_devops_sync_run SET status='FAILED',finished_at=now(),error=$2 WHERE id=$1",
        )
        .bind(run)
        .bind(error.chars().take(1000).collect::<String>())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("DELETE FROM ms_devops_stage WHERE run_id=$1")
            .bind(run)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(json!({"id":run,"status":"FAILED"}))
    }
    async fn dashboard(&self, source: &str, days: i64) -> Result<Value, Error> {
        let source_data = sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(s) FROM ms_devops_source s WHERE id=$1",
        )
        .bind(source)
        .fetch_optional(&self.0)
        .await
        .map_err(db)?
        .ok_or(Error::NotFound)?;
        let totals=sqlx::query_scalar::<_,Value>(&format!("SELECT jsonb_build_object('requirements',count(*) FILTER(WHERE kind='requirement'),'bugs',count(*) FILTER(WHERE kind='bug'),'sprints',count(*) FILTER(WHERE kind='sprint'),'delayed',count(*) FILTER(WHERE {LATE}),'delivered',count(*) FILTER(WHERE kind='requirement' AND payload->>'status'='已发布' AND NULLIF(payload->>'statusChangedAt','')::timestamptz>=now()-make_interval(days => $2::int))) FROM ms_devops_entity WHERE source_id=$1 AND active")).bind(source).bind(days as i32).fetch_one(&self.0).await.map_err(db)?;
        let authors=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('name',payload->>'author','group',payload->>'group','commits',count(*),'linkedCommits',count(*) FILTER(WHERE jsonb_array_length(COALESCE(payload->'requirementIds','[]'))>0),'additions',sum((payload->>'additions')::bigint),'deletions',sum((payload->>'deletions')::bigint),'unknownStats',count(*) FILTER(WHERE payload->>'additions' IS NULL OR payload->>'deletions' IS NULL)) FROM ms_devops_entity WHERE source_id=$1 AND active AND kind='commit' AND (payload->>'authoredAt')::timestamptz>=now()-make_interval(days => $2::int) GROUP BY payload->>'author',payload->>'group' ORDER BY count(*) DESC").bind(source).bind(days as i32).fetch_all(&self.0).await.map_err(db)?;
        let statuses=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('kind',kind,'status',payload->>'status','count',count(*)) FROM ms_devops_entity WHERE source_id=$1 AND active AND kind IN ('requirement','bug') GROUP BY kind,payload->>'status'").bind(source).fetch_all(&self.0).await.map_err(db)?;
        let runs=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(r) FROM ms_devops_sync_run r WHERE source_id=$1 ORDER BY started_at DESC LIMIT 10").bind(source).fetch_all(&self.0).await.map_err(db)?;
        Ok(
            json!({"source":source_data,"totals":totals,"authors":authors,"statuses":statuses,"runs":runs,"days":days,"codeScope":"组织可见仓库；关联提交按需求ID归属项目，未关联提交单独保留","historyScope":"首次同步建立基线，此后记录观察到的状态变化；不推算导入前历史"}),
        )
    }
    async fn records(&self, source: &str, q: ListQuery) -> Result<Value, Error> {
        let page = q.page.unwrap_or(1).max(1);
        let size = q.page_size.unwrap_or(30).clamp(1, 100);
        let kind = q.kind.unwrap_or_else(|| "requirement".into());
        if kind == "history" || kind == "runs" {
            let table = if kind == "history" { "ms_devops_history" } else { "ms_devops_sync_run" };
            let order = if kind == "history" { "observed_at" } else { "started_at" };
            let n = sqlx::query_scalar::<_, i64>(&format!(
                "SELECT count(*) FROM {table} WHERE source_id=$1"
            ))
            .bind(source)
            .fetch_one(&self.0)
            .await
            .map_err(db)?;
            let rows=sqlx::query_scalar::<_,Value>(&format!("SELECT to_jsonb(e) FROM {table} e WHERE source_id=$1 ORDER BY {order} DESC LIMIT $2 OFFSET $3")).bind(source).bind(size).bind((page-1)*size).fetch_all(&self.0).await.map_err(db)?;
            return Ok(json!({"total":n,"items":rows,"page":page,"pageSize":size}));
        }
        if !crate::domain::KINDS.contains(&kind.as_str()) {
            return Err(Error::Invalid("invalid record kind".into()));
        }
        let search = format!("%{}%", q.search.unwrap_or_default());
        let group = q.group.unwrap_or_default();
        let delayed = q.delayed_only.unwrap_or(false);
        let filters=format!("source_id=$1 AND active AND kind=$2 AND (payload->>'title' ILIKE $3 OR external_id ILIKE $3 OR payload->>'serial' ILIKE $3 OR payload->>'owner' ILIKE $3 OR payload->>'author' ILIKE $3) AND ($4='' OR payload->>'group'=$4) AND (NOT $5 OR ({LATE})) AND (kind<>'commit' OR (payload->>'authoredAt')::timestamptz>=now()-make_interval(days => $6::int))");
        let days = q.days.unwrap_or(7).clamp(1, 366) as i32;
        let n = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT count(*) FROM ms_devops_entity WHERE {filters}"
        ))
        .bind(source)
        .bind(&kind)
        .bind(&search)
        .bind(&group)
        .bind(delayed)
        .bind(days)
        .fetch_one(&self.0)
        .await
        .map_err(db)?;
        let rows=sqlx::query_scalar::<_,Value>(&format!("SELECT payload || jsonb_build_object('externalId',external_id,'observedAt',observed_at) FROM ms_devops_entity WHERE {filters} ORDER BY COALESCE(payload->>'authoredAt',payload->>'updatedAt',payload->>'serial',external_id) DESC,external_id LIMIT $7 OFFSET $8")).bind(source).bind(&kind).bind(&search).bind(&group).bind(delayed).bind(days).bind(size).bind((page-1)*size).fetch_all(&self.0).await.map_err(db)?;
        Ok(json!({"total":n,"items":rows,"page":page,"pageSize":size}))
    }
}
async fn lock_run(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    run: &str,
) -> Result<String, Error> {
    let r = sqlx::query("SELECT source_id,status FROM ms_devops_sync_run WHERE id=$1 FOR UPDATE")
        .bind(run)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or(Error::NotFound)?;
    if r.get::<String, _>("status") != "RUNNING" {
        return Err(Error::Conflict("run is not running".into()));
    }
    Ok(r.get("source_id"))
}

use crate::{
    domain::{
        build_member_metrics, CompleteRun, Error, ListQuery, MemberMetricsQuery, NewSource,
        ProductivityMetric, Record, RequirementOutputQuery,
    },
    ports::Store,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
    cache: Option<std::sync::Arc<super::statistics_cache::StatisticsCache>>,
    precomputing: bool,
}

/// Runs only against the plugin database; never uses Shepherd's migration inventory.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
pub(super) fn db(e: sqlx::Error) -> Error {
    tracing::error!(error=%e,"DevOps storage failed");
    Error::Storage
}
const LATE: &str = "kind='requirement' AND payload->>'priority' IN ('紧急','高','中') AND payload->>'status' NOT IN ('待发布','已发布','已交付','已拒绝','已取消','已完成','已关闭','已验收','完成') AND NULLIF(payload->>'due','')::date < (now() AT TIME ZONE 'Asia/Shanghai')::date";
#[async_trait]
impl Store for PgStore {
    async fn asset_export(&self, source: &str) -> Result<Value, Error> {
        sqlx::query_scalar("SELECT jsonb_build_object('sourceId',s.id,'revision',s.last_success_at::text,'records',COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',e.kind,'externalId',e.external_id,'data',e.payload-'raw') ORDER BY e.kind,e.external_id) FROM ms_devops_entity e WHERE e.source_id=s.id AND e.active AND e.kind IN ('requirement','testcase')),'[]'::jsonb)) FROM ms_devops_source s WHERE s.id=$1 AND s.last_success_at IS NOT NULL")
            .bind(source).fetch_optional(&self.pool).await.map_err(db)?.ok_or(Error::NotFound)
    }
    async fn activity_members(&self, source: &str) -> Result<Value, Error> {
        sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('members',COALESCE((SELECT jsonb_agg(jsonb_build_object('name',m.name,'group',m.group_name,'roles',m.roles,'enabled',m.enabled) ORDER BY m.name) FROM ms_devops_activity_member m WHERE m.source_id=s.id),'[]')) FROM ms_devops_source s WHERE s.id=$1")
            .bind(source).fetch_optional(&self.pool).await.map_err(db)?.ok_or(Error::NotFound)
    }
    async fn set_activity_members(
        &self,
        source: &str,
        config: crate::domain::team_activity::ActivityMembers,
    ) -> Result<Value, Error> {
        let config = config.normalize()?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query_scalar::<_, String>("SELECT id FROM ms_devops_source WHERE id=$1 FOR UPDATE")
            .bind(source)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or(Error::NotFound)?;
        sqlx::query("DELETE FROM ms_devops_activity_member WHERE source_id=$1")
            .bind(source)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for member in &config.members {
            let roles: Vec<_> = member.roles.iter().map(|r| r.as_str()).collect();
            sqlx::query("INSERT INTO ms_devops_activity_member(source_id,name,group_name,roles,enabled) VALUES($1,$2,$3,$4,$5)")
                .bind(source).bind(&member.name).bind(&member.group).bind(roles).bind(member.enabled).execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        self.warm_statistics(source);
        Ok(json!(config))
    }
    async fn activity_review_policy(&self, source: &str) -> Result<Value, Error> {
        sqlx::query_scalar::<_,Value>("SELECT COALESCE(p.policy,$2) FROM ms_devops_source s LEFT JOIN ms_devops_activity_review_policy p ON p.source_id=s.id WHERE s.id=$1")
            .bind(source).bind(json!(crate::domain::activity_review::ActivityReviewPolicy::default()))
            .fetch_optional(&self.pool).await.map_err(db)?.ok_or(Error::NotFound)
    }
    async fn set_activity_review_policy(
        &self,
        source: &str,
        policy: crate::domain::activity_review::ActivityReviewPolicy,
    ) -> Result<Value, Error> {
        policy.validate()?;
        let mut tx = self.pool.begin().await.map_err(db)?;
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
        let value = json!(policy);
        sqlx::query("INSERT INTO ms_devops_activity_review_policy(source_id,policy) VALUES($1,$2) ON CONFLICT(source_id) DO UPDATE SET policy=EXCLUDED.policy").bind(source).bind(&value).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.warm_statistics(source);
        Ok(value)
    }

    async fn team_activity(
        &self,
        source: &str,
        query: crate::domain::team_activity::ActivityQuery,
        permissions: crate::domain::team_activity::ActivityPermissions,
    ) -> Result<Value, Error> {
        query.validate()?;
        self.cached(
            source,
            "team-activity",
            format!("{query:?}/{permissions:?}"),
            self.team_activity_uncached(source, query, permissions),
        )
        .await
    }
    async fn author_identities(&self, source: &str) -> Result<Value, Error> {
        sqlx::query_scalar::<_, Value>(
            "SELECT jsonb_build_object('identities',COALESCE((SELECT jsonb_agg(jsonb_build_object('name',i.name,'group',i.group_name,'aliases',COALESCE((SELECT jsonb_agg(a.alias ORDER BY a.alias) FROM ms_devops_author_alias a WHERE a.source_id=i.source_id AND a.name=i.name AND a.alias<>i.name),'[]')) ORDER BY i.name) FROM ms_devops_author_identity i WHERE i.source_id=s.id),'[]')) FROM ms_devops_source s WHERE s.id=$1"
        ).bind(source).fetch_optional(&self.pool).await.map_err(db)?.ok_or(Error::NotFound)
    }
    async fn set_author_identities(
        &self,
        source: &str,
        config: crate::domain::author_identities::AuthorIdentities,
    ) -> Result<Value, Error> {
        let config = config.normalize()?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query_scalar::<_, String>("SELECT id FROM ms_devops_source WHERE id=$1 FOR UPDATE")
            .bind(source)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or(Error::NotFound)?;
        sqlx::query("DELETE FROM ms_devops_author_identity WHERE source_id=$1")
            .bind(source)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for identity in &config.identities {
            sqlx::query(
                "INSERT INTO ms_devops_author_identity(source_id,name,group_name) VALUES($1,$2,$3)",
            )
            .bind(source)
            .bind(&identity.name)
            .bind(&identity.group)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            for alias in std::iter::once(&identity.name).chain(identity.aliases.iter()) {
                sqlx::query(
                    "INSERT INTO ms_devops_author_alias(source_id,alias,name) VALUES($1,$2,$3)",
                )
                .bind(source)
                .bind(alias)
                .bind(&identity.name)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            }
        }
        tx.commit().await.map_err(db)?;
        self.warm_statistics(source);
        Ok(json!(config))
    }

    async fn sources(&self) -> Result<Value, Error> {
        let rows = sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(s) FROM ms_devops_source s ORDER BY s.name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(json!({"items":rows}))
    }
    async fn create_source(&self, s: NewSource) -> Result<Value, Error> {
        s.validate()?;
        let id = uuid::Uuid::new_v4().to_string();
        let row=sqlx::query_scalar::<_,Value>("INSERT INTO ms_devops_source(id,project_id,cloud_org_id,cloud_project_id,name) VALUES($1,$2,$3,$4,$5) ON CONFLICT(cloud_org_id,cloud_project_id) DO UPDATE SET name=EXCLUDED.name WHERE ms_devops_source.project_id=EXCLUDED.project_id RETURNING to_jsonb(ms_devops_source)")
   .bind(id).bind(s.project_id).bind(s.cloud_org_id).bind(s.cloud_project_id).bind(s.name).fetch_optional(&self.pool).await.map_err(db)?;
        row.ok_or(Error::Conflict(
            "cloud project already mapped to a different Shepherd project".into(),
        ))
    }
    async fn start(&self, source: &str) -> Result<Value, Error> {
        let mut tx = self.pool.begin().await.map_err(db)?;
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
        self.warm_statistics(source);
        Ok(json!({"id":id,"status":"RUNNING"}))
    }
    async fn stage(&self, run: &str, records: Vec<Record>) -> Result<Value, Error> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_run(&mut tx, run).await?;
        for r in &records {
            sqlx::query("INSERT INTO ms_devops_stage(run_id,kind,external_id,payload) VALUES($1,$2,$3,$4) ON CONFLICT(run_id,kind,external_id) DO UPDATE SET payload=EXCLUDED.payload").bind(run).bind(&r.kind).bind(&r.external_id).bind(&r.data).execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(json!({"staged":records.len()}))
    }
    async fn complete(&self, run: &str, b: CompleteRun) -> Result<Value, Error> {
        b.validate()?;
        let mut tx = self.pool.begin().await.map_err(db)?;
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
        // Refresh the successfully scanned code window; older facts remain available for history.
        sqlx::query("UPDATE ms_devops_entity e SET active=false,observed_at=now() WHERE e.source_id=$2 AND e.kind='commit' AND e.active AND (payload->>'authoredAt')::timestamptz >= $3::text::timestamptz AND (payload->>'authoredAt')::timestamptz < $4::text::timestamptz AND NOT EXISTS(SELECT 1 FROM ms_devops_stage s WHERE s.run_id=$1 AND s.kind='commit' AND s.external_id=e.external_id)").bind(run).bind(&source).bind(&b.code_since).bind(&b.code_until).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE ms_devops_source SET last_success_at=now(),code_since=LEAST(COALESCE(code_since,$2::text::timestamptz),$2::text::timestamptz),code_until=$3::text::timestamptz WHERE id=$1").bind(&source).bind(&b.code_since).bind(&b.code_until).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE ms_devops_sync_run SET status='SUCCESS',finished_at=now(),counts=$2 WHERE id=$1").bind(run).bind(json!(actual)).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM ms_devops_stage WHERE run_id=$1")
            .bind(run)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("ANALYZE ms_devops_statistics_entity").execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.warm_statistics(&source);
        Ok(json!({"id":run,"status":"SUCCESS","counts":actual}))
    }
    async fn fail(&self, run: &str, error: &str) -> Result<Value, Error> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let source = lock_run(&mut tx, run).await?;
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
        self.warm_statistics(&source);
        Ok(json!({"id":run,"status":"FAILED"}))
    }
    async fn bug_statistics(
        &self,
        source: &str,
        query: crate::domain::bug_statistics::BugStatisticsQuery,
    ) -> Result<Value, Error> {
        query.validate()?;
        self.cached(
            source,
            "bug-statistics",
            format!("{query:?}"),
            self.bug_statistics_uncached(source, query),
        )
        .await
    }
    async fn dashboard(&self, source: &str, days: i64) -> Result<Value, Error> {
        self.cached(source, "dashboard", format!("{days}"), self.dashboard_uncached(source, days))
            .await
    }
    async fn code_authors(
        &self,
        source: &str,
        query: crate::domain::code_authors::CodeAuthorsQuery,
        can_requirements: bool,
    ) -> Result<Value, Error> {
        let (start, end) = query.bounds()?;
        let source_data = sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(s) FROM ms_devops_source s WHERE id=$1",
        )
        .bind(source)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or(Error::NotFound)?;
        let rows=sqlx::query_as::<_,(String,Value)>("SELECT external_id,payload-'raw' FROM ms_devops_statistics_resolved WHERE source_id=$1 AND active AND kind='commit' AND NULLIF(payload->>'authoredAt','')::timestamptz >= $2::text::timestamptz AND NULLIF(payload->>'authoredAt','')::timestamptz < $3::text::timestamptz ORDER BY NULLIF(payload->>'authoredAt','')::timestamptz,external_id")
            .bind(source).bind(start.to_rfc3339()).bind(end.to_rfc3339()).fetch_all(&self.pool).await.map_err(db)?;
        let requirements = if can_requirements {
            sqlx::query_as::<_,(String,Value)>("SELECT external_id,jsonb_build_object('serial',payload->>'serial','title',payload->>'title') FROM ms_devops_statistics_resolved WHERE source_id=$1 AND active AND kind='requirement'").bind(source).fetch_all(&self.pool).await.map_err(db)?
                .into_iter().map(|(external_id,data)| Record {kind:"requirement".into(),external_id,data}).collect()
        } else {
            Vec::new()
        };
        let commits = rows
            .into_iter()
            .map(|(external_id, data)| Record { kind: "commit".into(), external_id, data })
            .collect();
        Ok(
            json!({"source":source_data,"month":query.month,"start":start.to_rfc3339(),"endExclusive":end.to_rfc3339(),"timezone":"Asia/Shanghai","commits":crate::domain::code_authors::facts(commits,requirements,can_requirements),"permissions":{"requirements":can_requirements}}),
        )
    }
    async fn member_metrics(
        &self,
        source: &str,
        query: MemberMetricsQuery,
    ) -> Result<Value, Error> {
        query.validate()?;
        self.cached(
            source,
            "member-metrics",
            format!("{query:?}"),
            self.member_metrics_uncached(source, query),
        )
        .await
    }
    async fn requirement_output(&self, q: RequirementOutputQuery) -> Result<Value, Error> {
        let project = q.project_id.unwrap_or_default();
        let source = q.source_id.unwrap_or_default();
        if (project.is_empty() && source.is_empty())
            || q.reference.trim().is_empty()
            || q.reference.len() > 300
        {
            return Err(Error::Invalid(
                "a source/project and exact requirement reference are required".into(),
            ));
        }
        let targets=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('source',to_jsonb(s),'requirement',(e.payload-'raw')||jsonb_build_object('externalId',e.external_id,'creator',COALESCE(NULLIF(e.payload->>'creator',''),NULLIF(e.payload->'raw'->'creator'->>'name',''),'待归属'))) FROM ms_devops_entity e JOIN ms_devops_source s ON s.id=e.source_id WHERE e.kind='requirement' AND e.active AND ($1='' OR s.id=$1) AND ($2='' OR s.project_id=$2) AND (e.external_id=$3 OR e.payload->>'serial'=$3) ORDER BY e.external_id LIMIT 2")
            .bind(source).bind(project).bind(&q.reference).fetch_all(&self.pool).await.map_err(db)?;
        if targets.len() > 1 {
            return Err(Error::Conflict("ambiguous reference; select a source".into()));
        }
        let mut result = targets.into_iter().next().ok_or(Error::NotFound)?;
        let source = result["source"]["id"].as_str().ok_or(Error::Storage)?;
        let id = result["requirement"]["externalId"].as_str().ok_or(Error::Storage)?;
        let serial = result["requirement"]["serial"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or(id);
        let size = q.page_size.unwrap_or(20).clamp(1, 100);
        let bug_page = q.bug_page.unwrap_or(1).max(1);
        let commit_page = q.commit_page.unwrap_or(1).max(1);
        let output = sqlx::query_scalar::<_, Value>(include_str!("requirement_output.sql"))
            .bind(source)
            .bind(id)
            .bind(serial)
            .bind(q.days.map(|days| days.clamp(1, 366)))
            .bind(size)
            .bind((bug_page - 1).saturating_mul(size))
            .bind((commit_page - 1).saturating_mul(size))
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        result["output"] = output;
        result["pageSize"] = json!(size);
        result["bugPage"] = json!(bug_page);
        result["commitPage"] = json!(commit_page);
        Ok(result)
    }
    async fn records(&self, source: &str, query: ListQuery) -> Result<Value, Error> {
        self.cached(source, "records", format!("{query:?}"), self.records_uncached(source, query))
            .await
    }
}
impl PgStore {
    async fn productivity_records(
        &self,
        source: &str,
        q: ListQuery,
        size: i64,
        page: i64,
    ) -> Result<Value, Error> {
        let metric = q
            .metric
            .ok_or_else(|| Error::Invalid("metric and date are required together".into()))?;
        let period = q.period.unwrap_or(false);
        if period && q.date.is_some() {
            return Err(Error::Invalid("choose a date or period, not both".into()));
        }
        let through = if period {
            let date = sqlx::query_scalar::<_, String>(
                "SELECT to_char((now() AT TIME ZONE 'Asia/Shanghai')::date,'YYYY-MM-DD')",
            )
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|_| Error::Storage)?
        } else {
            chrono::NaiveDate::parse_from_str(q.date.as_deref().unwrap_or(""), "%Y-%m-%d")
                .map_err(|_| Error::Invalid("invalid event date".into()))?
        };
        let from = through
            - chrono::Duration::days(if period {
                q.days.unwrap_or(7).clamp(1, 366) - 1
            } else {
                0
            });
        let kind = metric.record_kind();
        if q.kind.as_deref().is_some_and(|supplied| supplied != kind) {
            return Err(Error::Invalid("metric does not match record kind".into()));
        }
        let (field, condition) = match metric {
            ProductivityMetric::Delivered => ("statusChangedAt", "e.payload->>'status'='已发布'"),
            ProductivityMetric::Commits => ("authoredAt", "true"),
            ProductivityMetric::LinkedCommits | ProductivityMetric::Additions | ProductivityMetric::Deletions =>
                ("authoredAt", "jsonb_array_length(COALESCE(e.payload->'requirementIds','[]'))>0"),
            ProductivityMetric::UnknownStats => ("authoredAt", "jsonb_array_length(COALESCE(e.payload->'requirementIds','[]'))>0 AND (e.payload->>'additions' IS NULL OR e.payload->>'deletions' IS NULL)"),
            _ => ("createdAt", "true"),
        };
        let filters = format!("e.source_id=$1 AND e.active AND e.kind=$2 AND (NULLIF(e.payload->>'{field}','')::timestamptz AT TIME ZONE 'Asia/Shanghai')::date BETWEEN $3::date AND $5::date AND ({condition}) AND (COALESCE(e.payload->>'title','') ILIKE $4 OR e.external_id ILIKE $4 OR COALESCE(e.payload->>'serial','') ILIKE $4 OR e.payload->>'author' ILIKE $4 OR e.payload->>'authorRaw' ILIKE $4)");
        let search = format!("%{}%", q.search.unwrap_or_default());
        let count = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT count(*) FROM ms_devops_statistics_resolved e WHERE {filters}"
        ))
        .bind(source)
        .bind(kind)
        .bind(from.to_string())
        .bind(&search)
        .bind(through.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        // Resolve associations inside the plugin database and the same source, including unresolved IDs.
        let order = format!("(NULLIF(e.payload->>'{field}','')::timestamptz) DESC,e.external_id");
        let rows = sqlx::query_scalar::<_, Value>(&record_query(&filters, &order, "$6", "$7"))
            .bind(source)
            .bind(kind)
            .bind(from.to_string())
            .bind(&search)
            .bind(through.to_string())
            .bind(size)
            .bind((page - 1).saturating_mul(size))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(json!({"total":count,"items":rows,"page":page,"pageSize":size}))
    }
}

fn record_query(filters: &str, order: &str, limit: &str, offset: &str) -> String {
    format!(
        include_str!("record_select.sql"),
        filters = filters,
        order = order,
        limit = limit,
        offset = offset
    )
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

impl PgStore {
    pub fn start_statistics_warmer(&self) {
        if self.cache.is_none() {
            return;
        }
        let store = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let _ =
                    sqlx::query("DELETE FROM ms_devops_statistics_snapshot WHERE expires_at<now()")
                        .execute(&store.pool)
                        .await;
                if let Ok(sources) =
                    sqlx::query_scalar::<_, String>("SELECT id FROM ms_devops_source")
                        .fetch_all(&store.pool)
                        .await
                {
                    for source in sources {
                        store.warm_statistics(&source);
                    }
                }
            }
        });
    }
    fn warm_statistics(&self, source: &str) {
        if self.cache.is_none() {
            return;
        }
        let mut store = self.clone();
        store.precomputing = true;
        let source = source.to_owned();
        tokio::spawn(async move {
            let _ = store.dashboard(&source, 7).await;
            let _ = store
                .records(
                    &source,
                    ListQuery {
                        kind: Some("requirement".into()),
                        search: Some(String::new()),
                        page: Some(1),
                        page_size: Some(20),
                        ..Default::default()
                    },
                )
                .await;
            if let Ok(month) = sqlx::query_scalar::<_, String>(
                "SELECT to_char(now() AT TIME ZONE 'Asia/Shanghai','YYYY-MM')",
            )
            .fetch_one(&store.pool)
            .await
            {
                let _ = store
                    .member_metrics(
                        &source,
                        MemberMetricsQuery { month: Some(month), requirement_target: Some(10.0) },
                    )
                    .await;
            }
            let _ = store
                .bug_statistics(
                    &source,
                    crate::domain::bug_statistics::BugStatisticsQuery {
                        days: Some(7),
                        person_role: Some(
                            crate::domain::bug_statistics::BugPersonRole::Responsible,
                        ),
                        ..Default::default()
                    },
                )
                .await;
            let _ = store
                .team_activity(
                    &source,
                    crate::domain::team_activity::ActivityQuery {
                        days: Some(7),
                        ..Default::default()
                    },
                    crate::domain::team_activity::ActivityPermissions {
                        requirements: true,
                        bugs: true,
                        testcases: true,
                    },
                )
                .await;
        });
    }
    pub fn new(pool: PgPool) -> Self {
        Self { pool, cache: None, precomputing: false }
    }
    pub fn with_redis_url(mut self, url: Option<&str>) -> Self {
        self.cache = url.and_then(super::statistics_cache::StatisticsCache::new);
        self
    }
    async fn cached<F>(
        &self,
        source: &str,
        endpoint: &str,
        query: String,
        fetch: F,
    ) -> Result<Value, Error>
    where
        F: std::future::Future<Output = Result<Value, Error>>,
    {
        let Some(cache) = &self.cache else { return fetch.await };
        let Some(revision) = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(v.revision,0),floor((extract(epoch from now())+28800)/86400)::bigint FROM ms_devops_source s LEFT JOIN ms_devops_statistics_state v ON v.source_id=s.id WHERE s.id=$1",
        ).bind(source).fetch_optional(&self.pool).await.map_err(db)? else {return fetch.await};
        let key = super::statistics_cache::StatisticsCache::key(
            source, revision.0, revision.1, endpoint, &query,
        );
        if !self.precomputing {
            if let Some(value) = cache.get(&key).await {
                return Ok(value);
            }
        }
        let flight = cache.flight(&key).await;
        let _guard = flight.lock().await;
        if !self.precomputing {
            if let Some(value) = cache.get(&key).await {
                return Ok(value);
            }
            if let Ok(Some(value))=sqlx::query_as::<_,(Value,i64)>("SELECT payload,GREATEST(1,ceil(extract(epoch from expires_at-now())))::bigint FROM ms_devops_statistics_snapshot WHERE cache_key=$1 AND expires_at>now()")
                .bind(&key).fetch_optional(&self.pool).await {
                cache.set_for(&key,&value.0,value.1.clamp(1,90) as u64).await;return Ok(value.0);
            }
        }
        let value = fetch.await?;
        if self.precomputing {
            self.save_snapshot(source, revision.0, &key, &value).await;
        }
        cache.set(&key, &value).await;
        Ok(value)
    }
    async fn save_snapshot(&self, source: &str, revision: i64, key: &str, value: &Value) {
        if serde_json::to_vec(value).map_or(true, |v| v.len() > 4_000_000) {
            return;
        }
        let _=sqlx::query("INSERT INTO ms_devops_statistics_snapshot(cache_key,source_id,revision,payload) SELECT $1,$2,$3,$4 FROM ms_devops_statistics_state WHERE source_id=$2 AND revision=$3 ON CONFLICT(cache_key) DO UPDATE SET payload=EXCLUDED.payload,expires_at=now()+interval '90 seconds'")
            .bind(key).bind(source).bind(revision).bind(value).execute(&self.pool).await;
    }

    async fn dashboard_uncached(&self, source: &str, days: i64) -> Result<Value, Error> {
        let source_data = sqlx::query_scalar::<_, Value>(
            "SELECT to_jsonb(s) FROM ms_devops_source s WHERE id=$1",
        )
        .bind(source)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or(Error::NotFound)?;
        let totals=sqlx::query_scalar::<_,Value>(&format!("SELECT jsonb_build_object('requirements',count(*) FILTER(WHERE kind='requirement'),'bugs',count(*) FILTER(WHERE kind='bug'),'sprints',count(*) FILTER(WHERE kind='sprint'),'delayed',count(*) FILTER(WHERE {LATE}),'delivered',count(*) FILTER(WHERE kind='requirement' AND payload->>'status'='已发布' AND (NULLIF(payload->>'statusChangedAt','')::timestamptz AT TIME ZONE 'Asia/Shanghai')::date BETWEEN (now() AT TIME ZONE 'Asia/Shanghai')::date-$2::int+1 AND (now() AT TIME ZONE 'Asia/Shanghai')::date)) FROM ms_devops_statistics_resolved WHERE source_id=$1 AND active")).bind(source).bind(days as i32).fetch_one(&self.pool).await.map_err(db)?;
        let authors=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('name',payload->>'author','group',payload->>'group','commits',count(*),'linkedCommits',count(*) FILTER(WHERE jsonb_array_length(COALESCE(payload->'requirementIds','[]'))>0),'additions',sum((payload->>'additions')::bigint),'deletions',sum((payload->>'deletions')::bigint),'unknownStats',count(*) FILTER(WHERE payload->>'additions' IS NULL OR payload->>'deletions' IS NULL)) FROM ms_devops_statistics_resolved WHERE source_id=$1 AND active AND kind='commit' AND (payload->>'authoredAt')::timestamptz>=now()-make_interval(days => $2::int) GROUP BY payload->>'author',payload->>'group' ORDER BY count(*) DESC").bind(source).bind(days as i32).fetch_all(&self.pool).await.map_err(db)?;
        let groups=sqlx::query_scalar::<_,String>("SELECT DISTINCT payload->>'group' FROM ms_devops_statistics_resolved WHERE source_id=$1 AND active AND NULLIF(payload->>'group','') IS NOT NULL ORDER BY 1").bind(source).fetch_all(&self.pool).await.map_err(db)?;
        let statuses=sqlx::query_scalar::<_,Value>("SELECT jsonb_build_object('kind',kind,'status',payload->>'status','count',count(*)) FROM ms_devops_statistics_resolved WHERE source_id=$1 AND active AND kind IN ('requirement','bug') GROUP BY kind,payload->>'status'").bind(source).fetch_all(&self.pool).await.map_err(db)?;
        let productivity = sqlx::query_scalar::<_, Value>(include_str!("productivity.sql"))
            .bind(source)
            .bind(days as i32)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        let bug_statistics = sqlx::query_scalar::<_, Value>(include_str!("bug_statistics.sql"))
            .bind(source)
            .bind(days as i32)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        let runs=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(r) FROM ms_devops_sync_run r WHERE source_id=$1 ORDER BY started_at DESC LIMIT 10").bind(source).fetch_all(&self.pool).await.map_err(db)?;
        Ok(
            json!({"source":source_data,"productivity":productivity,"bugStatistics":bug_statistics,"totals":totals,"authors":authors,"statuses":statuses,"groups":groups,"runs":runs,"days":days,"codeScope":"组织可见仓库；关联提交按需求ID归属项目，未关联提交单独保留","historyScope":"首次同步建立基线，此后记录观察到的状态变化；不推算导入前历史"}),
        )
    }
    async fn records_uncached(&self, source: &str, q: ListQuery) -> Result<Value, Error> {
        let page = q.page.unwrap_or(1).max(1);
        let size = q.page_size.unwrap_or(30).clamp(1, 100);
        if q.metric.is_some() || q.date.is_some() || q.period.unwrap_or(false) {
            return self.productivity_records(source, q, size, page).await;
        }
        let kind = q.kind.unwrap_or_else(|| "requirement".into());
        if kind == "history" || kind == "runs" {
            let table = if kind == "history" { "ms_devops_history" } else { "ms_devops_sync_run" };
            let order = if kind == "history" { "observed_at" } else { "started_at" };
            let n = sqlx::query_scalar::<_, i64>(&format!(
                "SELECT count(*) FROM {table} WHERE source_id=$1"
            ))
            .bind(source)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
            let rows=sqlx::query_scalar::<_,Value>(&format!("SELECT to_jsonb(e) FROM {table} e WHERE source_id=$1 ORDER BY {order} DESC LIMIT $2 OFFSET $3")).bind(source).bind(size).bind((page-1)*size).fetch_all(&self.pool).await.map_err(db)?;
            return Ok(json!({"total":n,"items":rows,"page":page,"pageSize":size}));
        }
        if !crate::domain::KINDS.contains(&kind.as_str()) {
            return Err(Error::Invalid("invalid record kind".into()));
        }
        let search = format!("%{}%", q.search.unwrap_or_default());
        let group = q.group.unwrap_or_default();
        let delayed = q.delayed_only.unwrap_or(false);
        let status = q.status.unwrap_or_default();
        let filters=format!("source_id=$1 AND active AND kind=$2 AND (payload->>'title' ILIKE $3 OR external_id ILIKE $3 OR payload->>'serial' ILIKE $3 OR payload->>'owner' ILIKE $3 OR payload->>'author' ILIKE $3 OR payload->>'authorRaw' ILIKE $3) AND ($4='' OR payload->>'group'=$4) AND (NOT $5 OR ({LATE})) AND ($7='' OR payload->>'status'=$7) AND (kind<>'commit' OR (payload->>'authoredAt')::timestamptz>=now()-make_interval(days => $6::int))");
        let days = q.days.unwrap_or(7).clamp(1, 366) as i32;
        let n = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT count(*) FROM ms_devops_statistics_resolved WHERE {filters}"
        ))
        .bind(source)
        .bind(&kind)
        .bind(&search)
        .bind(&group)
        .bind(delayed)
        .bind(days)
        .bind(&status)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let rows=sqlx::query_scalar::<_,Value>(&record_query(&filters,"COALESCE(e.payload->>'authoredAt',e.payload->>'updatedAt',e.payload->>'serial',e.external_id) DESC,e.external_id","$8","$9"))
            .bind(source).bind(&kind).bind(&search).bind(&group).bind(delayed).bind(days).bind(&status).bind(size).bind((page-1).saturating_mul(size)).fetch_all(&self.pool).await.map_err(db)?;
        Ok(json!({"total":n,"items":rows,"page":page,"pageSize":size}))
    }
    async fn member_metrics_uncached(
        &self,
        source: &str,
        query: MemberMetricsQuery,
    ) -> Result<Value, Error> {
        query.validate()?;
        let snapshot = sqlx::query_scalar::<_, Value>(include_str!("member_metrics.sql"))
            .bind(source)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(Error::NotFound)?;
        let records: Vec<Record> =
            serde_json::from_value(snapshot["records"].clone()).map_err(|_| Error::Storage)?;
        let now =
            chrono::DateTime::parse_from_rfc3339(snapshot["now"].as_str().ok_or(Error::Storage)?)
                .map_err(|_| Error::Storage)?;
        let data = build_member_metrics(&records, &query, now)?;
        Ok(json!({"source":snapshot["source"],"metrics":data}))
    }
    async fn bug_statistics_uncached(
        &self,
        source: &str,
        query: crate::domain::bug_statistics::BugStatisticsQuery,
    ) -> Result<Value, Error> {
        query.validate()?;
        let snapshot = sqlx::query_scalar::<_, Value>(include_str!("bug_statistics_snapshot.sql"))
            .bind(source)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(Error::NotFound)?;
        let records: Vec<Record> =
            serde_json::from_value(snapshot["records"].clone()).map_err(|_| Error::Storage)?;
        let identities = serde_json::from_value(self.author_identities(source).await?)
            .map_err(|_| Error::Storage)?;
        let roster = serde_json::from_value(self.activity_members(source).await?)
            .map_err(|_| Error::Storage)?;
        let now =
            chrono::DateTime::parse_from_rfc3339(snapshot["now"].as_str().ok_or(Error::Storage)?)
                .map_err(|_| Error::Storage)?;
        crate::domain::bug_statistics::build(
            &snapshot["source"],
            &query,
            now,
            &records,
            &identities,
            &roster,
        )
    }
    async fn team_activity_uncached(
        &self,
        source: &str,
        query: crate::domain::team_activity::ActivityQuery,
        permissions: crate::domain::team_activity::ActivityPermissions,
    ) -> Result<Value, Error> {
        query.validate()?;
        let snapshot = sqlx::query_scalar::<_, Value>(include_str!("team_activity.sql"))
            .bind(source)
            .bind(query.role.as_str())
            .bind(permissions.requirements)
            .bind(permissions.bugs)
            .bind(permissions.testcases)
            .bind(query.days.unwrap_or(7) as i32)
            .bind(json!(crate::domain::activity_review::ActivityReviewPolicy::default()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(Error::NotFound)?;
        let records: Vec<Record> =
            serde_json::from_value(snapshot["records"].clone()).map_err(|_| Error::Storage)?;
        let identities = crate::domain::author_identities::AuthorIdentities {
            identities: serde_json::from_value(snapshot["identities"].clone())
                .map_err(|_| Error::Storage)?,
        };
        let roster = crate::domain::team_activity::ActivityMembers {
            members: serde_json::from_value(snapshot["members"].clone())
                .map_err(|_| Error::Storage)?,
        };
        let now =
            chrono::DateTime::parse_from_rfc3339(snapshot["now"].as_str().ok_or(Error::Storage)?)
                .map_err(|_| Error::Storage)?;
        let since = snapshot["collectedSince"]
            .as_str()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok());
        let mut activity = crate::domain::team_activity::build_activity(
            &records,
            &snapshot["source"],
            &query,
            &identities,
            &roster,
            permissions,
            crate::domain::team_activity::CollectionWindow {
                author_groups: serde_json::from_value(snapshot["authorGroups"].clone())
                    .map_err(|_| Error::Storage)?,
                now,
                collected_since: since,
                testcases_collected: snapshot["testcasesCollected"].as_bool().unwrap_or(false),
            },
        )?;
        activity["reviewPolicy"] = snapshot["reviewPolicy"].clone();
        Ok(activity)
    }
}

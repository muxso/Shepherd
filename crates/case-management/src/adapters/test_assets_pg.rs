use crate::domain::test_assets::*;
use crate::ports::test_assets::AssetStore;
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::PgPool;

#[derive(Clone)]
pub struct PgAssetStore(pub PgPool);
pub(super) fn db(e: sqlx::Error) -> AssetError {
    tracing::error!(error=%e,"Test asset storage failed");
    if e.as_database_error().is_some_and(|e| e.is_unique_violation()) {
        AssetError::Conflict("name or source binding already exists".into())
    } else {
        AssetError::Storage
    }
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
async fn service_exists(pool: &PgPool, project: &str, id: &str) -> AssetResult<()> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_test_service WHERE id=$1 AND project_id=$2 AND NOT deleted)").bind(id).bind(project).fetch_one(pool).await.map_err(db)?;
    if exists {
        Ok(())
    } else {
        Err(AssetError::NotFound)
    }
}
#[async_trait]
impl AssetStore for PgAssetStore {
    async fn set_case_api(
        &self,
        project: &str,
        case_id: &str,
        links: &[Value],
    ) -> AssetResult<Value> {
        if links.len() > 200 {
            return Err(AssetError::Invalid("at most 200 API links".into()));
        }
        let mut tx = self.0.begin().await.map_err(db)?;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_functional_case WHERE project_id=$1 AND id=$2 AND NOT deleted)").bind(project).bind(case_id).fetch_one(&mut *tx).await.map_err(db)?;
        if !valid {
            return Err(AssetError::NotFound);
        }
        sqlx::query("DELETE FROM ms_test_case_api WHERE project_id=$1 AND case_id=$2")
            .bind(project)
            .bind(case_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for link in links {
            let definition = link["apiDefinitionId"].as_str().filter(|s| !s.is_empty());
            let api_case = link["apiCaseId"].as_str().filter(|s| !s.is_empty());
            if definition.is_none() && api_case.is_none() {
                return Err(AssetError::Invalid("API definition or case required".into()));
            }
            if let Some(id) = definition {
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_api_definition WHERE project_id=$1 AND id=$2 AND NOT deleted)").bind(project).bind(id).fetch_one(&mut *tx).await.map_err(db)?;
                if !valid {
                    return Err(AssetError::Invalid("API definition outside project".into()));
                }
            }
            if let Some(id) = api_case {
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_api_case WHERE project_id=$1 AND id=$2 AND ($3::text IS NULL OR api_definition_id=$3))").bind(project).bind(id).bind(definition).fetch_one(&mut *tx).await.map_err(db)?;
                if !valid {
                    return Err(AssetError::Invalid(
                        "API case outside definition or project".into(),
                    ));
                }
            }
            sqlx::query("INSERT INTO ms_test_case_api(project_id,case_id,api_definition_id,api_case_id) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(project).bind(case_id).bind(definition).bind(api_case).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE ms_functional_case SET updated_at=now() WHERE project_id=$1 AND id=$2")
            .bind(project)
            .bind(case_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(json!({"saved":true}))
    }

    async fn authorize(&self, project: &str, user: &str, admin: bool) -> AssetResult<()> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_project p WHERE p.id=$1 AND NOT p.deleted AND ($3 OR EXISTS(SELECT 1 FROM ms_project_member m WHERE m.project_id=p.id AND m.user_id=$2)))").bind(project).bind(user).bind(admin).fetch_one(&self.0).await.map_err(db)?;
        if exists {
            Ok(())
        } else {
            Err(AssetError::Forbidden)
        }
    }
    async fn overview(&self, project: &str) -> AssetResult<Value> {
        let services: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(s)-'deleted' FROM ms_test_service s WHERE project_id=$1 AND NOT deleted ORDER BY name,id").bind(project).fetch_all(&self.0).await.map_err(db)?;
        let mappings: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(m) FROM ms_test_source_mapping m JOIN ms_test_service s ON s.id=m.service_id AND NOT s.deleted WHERE m.project_id=$1 ORDER BY m.scope,m.external_id").bind(project).fetch_all(&self.0).await.map_err(db)?;
        let cases: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',c.id,'name',c.name,'updatedAt',c.updated_at,'module',c.module,'serviceId',a.service_id,'sourceId',COALESCE(o.source_id,to_jsonb(c)->>'external_source_id'),'externalId',COALESCE(o.external_id,to_jsonb(c)->>'external_id'),'libraryId',COALESCE(o.source_payload->>'repoId',''),'library',COALESCE(o.source_payload->>'repo',c.custom_fields->>'云效用例库',''),'directory',COALESCE(o.source_payload->>'directory',c.custom_fields->>'云效目录',''),'sourceAvailable',o.source_present,'sourceRevision',o.source_revision) FROM ms_functional_case c LEFT JOIN ms_test_case_service a ON a.case_id=c.id AND a.project_id=c.project_id LEFT JOIN ms_test_asset_origin o ON o.kind='testcase' AND o.native_id=c.id AND o.project_id=c.project_id WHERE c.project_id=$1 AND NOT c.deleted ORDER BY c.num,c.id").bind(project).fetch_all(&self.0).await.map_err(db)?;
        let mut scenarios: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(s)-'deleted' FROM ms_functional_scenario s WHERE project_id=$1 AND NOT deleted ORDER BY updated_at DESC,id").bind(project).fetch_all(&self.0).await.map_err(db)?;
        for s in &mut scenarios {
            let ids: Vec<String> = s["steps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|step| step["caseId"].as_str().map(str::to_owned))
                .collect();
            let now: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('caseId',id,'updatedAt',updated_at) ORDER BY id),'[]'::jsonb) FROM ms_functional_case WHERE project_id=$1 AND id=ANY($2) AND NOT deleted").bind(project).bind(ids).fetch_one(&self.0).await.map_err(db)?;
            let api:Value=sqlx::query_scalar("SELECT COALESCE((SELECT to_jsonb(updated_at) FROM ms_api_scenario WHERE project_id=$1 AND id=$2 AND NOT deleted),'null'::jsonb)").bind(project).bind(s["api_scenario_id"].as_str()).fetch_one(&self.0).await.map_err(db)?;
            s["stale"] = json!(
                s["status"] == "REVIEWED"
                    && (s["definition_snapshot"]["cases"] != now
                        || s["definition_snapshot"]["apiScenario"] != api)
            );
        }
        let inventories: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(i) FROM ms_test_inventory i WHERE project_id=$1 ORDER BY created_at DESC,id").bind(project).fetch_all(&self.0).await.map_err(db)?;
        let imports: Vec<Value> = sqlx::query_scalar("SELECT jsonb_set(to_jsonb(b)-'records','{preview}',jsonb_build_object('counts',b.preview->'counts','manualAssets',b.preview->'manualAssets')) FROM ms_test_import b WHERE project_id=$1 ORDER BY created_at DESC LIMIT 20").bind(project).fetch_all(&self.0).await.map_err(db)?;
        let mut cases = cases;
        for c in &mut cases {
            if c["serviceId"].is_string() {
                continue;
            }
            let found = mappings
                .iter()
                .filter(|m| text(m, "source_id") == text(c, "sourceId"))
                .filter_map(|m| {
                    let rank = match text(m, "scope") {
                        "case" if text(m, "external_id") == text(c, "externalId") => 3,
                        "directory" if text(m, "external_id") == text(c, "directory") => 2,
                        "library" if text(m, "external_id") == text(c, "libraryId") => 1,
                        _ => return None,
                    };
                    Some((rank, text(m, "service_id")))
                })
                .max_by_key(|(rank, _)| *rank);
            if let Some((_, id)) = found {
                c["serviceId"] = json!(id);
            }
        }
        let api_links: Vec<Value> = sqlx::query_scalar(
            "SELECT to_jsonb(l) FROM ms_test_case_api l WHERE project_id=$1 ORDER BY case_id,id",
        )
        .bind(project)
        .fetch_all(&self.0)
        .await
        .map_err(db)?;
        Ok(
            json!({"apiLinks":api_links,"services":services,"cases":cases,"mappings":mappings,"scenarios":scenarios,"inventories":inventories,"imports":imports}),
        )
    }
    async fn save_service(
        &self,
        project: &str,
        id: Option<&str>,
        input: &ServiceInput,
    ) -> AssetResult<Value> {
        input.validate()?;
        let repos = serde_json::to_value(&input.repositories).map_err(|_| AssetError::Storage)?;
        let result = if let Some(id) = id {
            sqlx::query_scalar::<_,Value>("UPDATE ms_test_service SET name=$3,description=$4,repositories=$5,updated_at=now() WHERE project_id=$1 AND id=$2 AND NOT deleted RETURNING to_jsonb(ms_test_service)").bind(project).bind(id).bind(input.name.trim()).bind(&input.description).bind(repos).fetch_optional(&self.0).await.map_err(db)?
        } else {
            sqlx::query_scalar::<_,Value>("INSERT INTO ms_test_service(project_id,name,description,repositories) VALUES($1,$2,$3,$4) RETURNING to_jsonb(ms_test_service)").bind(project).bind(input.name.trim()).bind(&input.description).bind(repos).fetch_optional(&self.0).await.map_err(db)?
        };
        result.ok_or(AssetError::NotFound)
    }
    async fn delete_service(&self, project: &str, id: &str) -> AssetResult<()> {
        let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_test_case_service WHERE project_id=$1 AND service_id=$2 UNION ALL SELECT 1 FROM ms_test_source_mapping WHERE project_id=$1 AND service_id=$2)").bind(project).bind(id).fetch_one(&self.0).await.map_err(db)?;
        if used {
            return Err(AssetError::Conflict(
                "reassign cases and source mappings before removing this service".into(),
            ));
        }
        service_exists(&self.0, project, id).await?;
        sqlx::query("UPDATE ms_test_service SET deleted=true,updated_at=now() WHERE project_id=$1 AND id=$2").bind(project).bind(id).execute(&self.0).await.map_err(db)?;
        Ok(())
    }
    async fn assign_cases(
        &self,
        project: &str,
        service: Option<&str>,
        cases: &[String],
    ) -> AssetResult<()> {
        if cases.is_empty() || cases.len() > 10000 {
            return Err(AssetError::Invalid("choose 1..10000 cases".into()));
        }
        let mut tx = self.0.begin().await.map_err(db)?;
        if let Some(service) = service {
            service_exists(&self.0, project, service).await?;
        }
        let n: i64=sqlx::query_scalar("SELECT count(DISTINCT id) FROM ms_functional_case WHERE project_id=$1 AND id=ANY($2) AND NOT deleted").bind(project).bind(cases).fetch_one(&mut *tx).await.map_err(db)?;
        if n != cases.iter().collect::<std::collections::BTreeSet<_>>().len() as i64 {
            return Err(AssetError::Invalid("case is outside the project".into()));
        }
        sqlx::query("DELETE FROM ms_test_case_service WHERE project_id=$1 AND case_id=ANY($2)")
            .bind(project)
            .bind(cases)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if let Some(service) = service {
            sqlx::query("INSERT INTO ms_test_case_service(project_id,case_id,service_id) SELECT $1,id,$3 FROM ms_functional_case WHERE project_id=$1 AND id=ANY($2) AND NOT deleted").bind(project).bind(cases).bind(service).execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(())
    }
    async fn save_mapping(&self, project: &str, v: &Value) -> AssetResult<()> {
        let (source, scope, external, service) =
            (text(v, "sourceId"), text(v, "scope"), text(v, "externalId"), text(v, "serviceId"));
        bounded(source, 255, true)?;
        bounded(external, 2000, true)?;
        if !matches!(scope, "case" | "directory" | "library") {
            return Err(AssetError::Invalid("invalid mapping scope".into()));
        }
        if service.is_empty() {
            sqlx::query("DELETE FROM ms_test_source_mapping WHERE project_id=$1 AND source_id=$2 AND scope=$3 AND external_id=$4").bind(project).bind(source).bind(scope).bind(external).execute(&self.0).await.map_err(db)?;
        } else {
            service_exists(&self.0, project, service).await?;
            sqlx::query("INSERT INTO ms_test_source_mapping(project_id,source_id,scope,external_id,service_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT(project_id,source_id,scope,external_id) DO UPDATE SET service_id=EXCLUDED.service_id").bind(project).bind(source).bind(scope).bind(external).bind(service).execute(&self.0).await.map_err(db)?;
        }
        Ok(())
    }
    async fn preview_import(
        &self,
        project: &str,
        user: &str,
        snapshot: &SourceSnapshot,
    ) -> AssetResult<Value> {
        super::test_assets_import::preview(&self.0, project, user, snapshot).await
    }
    async fn import_item(
        &self,
        project: &str,
        id: &str,
        kind: &str,
        external_id: &str,
    ) -> AssetResult<Value> {
        sqlx::query_scalar("SELECT item FROM ms_test_import b,jsonb_array_elements(b.preview->'items') item WHERE b.project_id=$1 AND b.id=$2 AND item->>'kind'=$3 AND item->>'externalId'=$4").bind(project).bind(id).bind(kind).bind(external_id).fetch_optional(&self.0).await.map_err(db)?.ok_or(AssetError::NotFound)
    }
    async fn import_batch(&self, project: &str, id: &str) -> AssetResult<Value> {
        sqlx::query_scalar(
            "SELECT to_jsonb(b)-'records' FROM ms_test_import b WHERE project_id=$1 AND id=$2",
        )
        .bind(project)
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .map_err(db)?
        .ok_or(AssetError::NotFound)
    }
    async fn apply_import(
        &self,
        project: &str,
        user: &str,
        id: &str,
        revision: &str,
        resolutions: &Value,
    ) -> AssetResult<Value> {
        super::test_assets_import::apply(&self.0, project, user, id, revision, resolutions).await
    }
    async fn save_scenario(
        &self,
        project: &str,
        user: &str,
        id: Option<&str>,
        input: &ScenarioInput,
    ) -> AssetResult<Value> {
        input.validate()?;
        let cases: Vec<_> = input.steps.iter().map(|s| s.case_id.clone()).collect();
        let n: i64=sqlx::query_scalar("SELECT count(*) FROM ms_functional_case WHERE project_id=$1 AND id=ANY($2) AND NOT deleted").bind(project).bind(&cases).fetch_one(&self.0).await.map_err(db)?;
        if n != cases.iter().collect::<std::collections::BTreeSet<_>>().len() as i64 {
            return Err(AssetError::Invalid("scenario case is outside the project".into()));
        }
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM ms_requirement WHERE project_id=$1 AND id=ANY($2) AND NOT deleted").bind(project).bind(&input.requirement_ids).fetch_one(&self.0).await.map_err(db)?;
        if n != input.requirement_ids.iter().collect::<std::collections::BTreeSet<_>>().len() as i64
        {
            return Err(AssetError::Invalid("scenario requirement is outside the project".into()));
        }
        if let Some(api) = input.api_scenario_id.as_deref().filter(|s| !s.is_empty()) {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_api_scenario WHERE project_id=$1 AND id=$2 AND NOT deleted)").bind(project).bind(api).fetch_one(&self.0).await.map_err(db)?;
            if !exists {
                return Err(AssetError::Invalid("API scenario is outside the project".into()));
            }
        }
        let definitions: Value = sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('caseId',id,'updatedAt',updated_at) ORDER BY id),'[]'::jsonb) FROM ms_functional_case WHERE project_id=$1 AND id=ANY($2) AND NOT deleted").bind(project).bind(&cases).fetch_one(&self.0).await.map_err(db)?;
        let api_version:Value=sqlx::query_scalar("SELECT COALESCE((SELECT to_jsonb(updated_at) FROM ms_api_scenario WHERE project_id=$1 AND id=$2 AND NOT deleted),'null'::jsonb)").bind(project).bind(&input.api_scenario_id).fetch_one(&self.0).await.map_err(db)?;
        let definition_snapshot = json!({"cases":definitions,"apiScenario":api_version});
        let steps = serde_json::to_value(&input.steps).map_err(|_| AssetError::Storage)?;
        let value = if let Some(id) = id {
            sqlx::query_scalar::<_,Value>("UPDATE ms_functional_scenario SET name=$3,description=$4,preconditions=$5,steps=$6,requirement_ids=$7,api_scenario_id=$8,status=$9,definition_snapshot=$10,revision=revision+1,updated_at=now() WHERE project_id=$1 AND id=$2 AND NOT deleted RETURNING to_jsonb(ms_functional_scenario)").bind(project).bind(id).bind(input.name.trim()).bind(&input.description).bind(&input.preconditions).bind(steps).bind(&input.requirement_ids).bind(input.api_scenario_id.as_deref().filter(|s|!s.is_empty())).bind(&input.status).bind(&definition_snapshot).fetch_optional(&self.0).await.map_err(db)?
        } else {
            sqlx::query_scalar::<_,Value>("INSERT INTO ms_functional_scenario(project_id,name,description,preconditions,steps,requirement_ids,api_scenario_id,status,created_by,definition_snapshot) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING to_jsonb(ms_functional_scenario)").bind(project).bind(input.name.trim()).bind(&input.description).bind(&input.preconditions).bind(steps).bind(&input.requirement_ids).bind(input.api_scenario_id.as_deref().filter(|s|!s.is_empty())).bind(&input.status).bind(user).bind(&definition_snapshot).fetch_optional(&self.0).await.map_err(db)?
        };
        value.ok_or(AssetError::NotFound)
    }
    async fn delete_scenario(&self, project: &str, id: &str) -> AssetResult<()> {
        let n=sqlx::query("UPDATE ms_functional_scenario SET deleted=true WHERE project_id=$1 AND id=$2 AND NOT deleted").bind(project).bind(id).execute(&self.0).await.map_err(db)?.rows_affected();
        if n == 0 {
            Err(AssetError::NotFound)
        } else {
            Ok(())
        }
    }
    async fn record_run(
        &self,
        project: &str,
        user: &str,
        id: &str,
        v: &Value,
    ) -> AssetResult<Value> {
        let mut tx = self.0.begin().await.map_err(db)?;
        let snapshot:Value=sqlx::query_scalar("SELECT to_jsonb(s) FROM ms_functional_scenario s WHERE project_id=$1 AND id=$2 AND NOT deleted FOR SHARE").bind(project).bind(id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or(AssetError::NotFound)?;
        if text(&snapshot, "status") != "REVIEWED" || v["revision"] != snapshot["revision"] {
            return Err(AssetError::Conflict(
                "review the current scenario revision before execution".into(),
            ));
        }
        let case_ids: Vec<String> = snapshot["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s["caseId"].as_str().map(str::to_owned))
            .collect();
        sqlx::query(
            "SELECT id FROM ms_functional_case WHERE project_id=$1 AND id=ANY($2) FOR SHARE",
        )
        .bind(project)
        .bind(&case_ids)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let current:Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('caseId',id,'updatedAt',updated_at) ORDER BY id),'[]'::jsonb) FROM ms_functional_case WHERE project_id=$1 AND id=ANY($2) AND NOT deleted").bind(project).bind(&case_ids).fetch_one(&mut *tx).await.map_err(db)?;
        if snapshot["definition_snapshot"]["cases"] != current {
            return Err(AssetError::Conflict(
                "referenced definitions changed; review the scenario again".into(),
            ));
        }
        if let Some(api) = snapshot["api_scenario_id"].as_str() {
            let current:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(updated_at) FROM ms_api_scenario WHERE project_id=$1 AND id=$2 AND NOT deleted FOR SHARE").bind(project).bind(api).fetch_optional(&mut *tx).await.map_err(db)?;
            if current.as_ref() != Some(&snapshot["definition_snapshot"]["apiScenario"]) {
                return Err(AssetError::Conflict(
                    "API scenario changed; review the scenario again".into(),
                ));
            }
        }
        bounded(text(v, "environment"), 500, true)?;
        bounded(text(v, "notes"), 16000, false)?;
        let steps = v["steps"]
            .as_array()
            .ok_or_else(|| AssetError::Invalid("step outcomes are required".into()))?;
        let expected = snapshot["steps"].as_array().ok_or(AssetError::Storage)?;
        if steps.len() != expected.len()
            || !steps.iter().all(|s| {
                matches!(text(s, "outcome"), "PASSED" | "FAILED" | "BLOCKED")
                    && !text(s, "actual").trim().is_empty()
                    && text(s, "actual").len() <= 16000
            })
        {
            return Err(AssetError::Invalid(
                "every step requires an outcome and actual result".into(),
            ));
        }
        let outcome = if steps.iter().any(|s| text(s, "outcome") == "FAILED") {
            "FAILED"
        } else if steps.iter().any(|s| text(s, "outcome") == "BLOCKED") {
            "BLOCKED"
        } else {
            "PASSED"
        };
        let row=sqlx::query_scalar("INSERT INTO ms_functional_scenario_run(project_id,scenario_id,revision,snapshot,steps,environment,outcome,notes,executed_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING to_jsonb(ms_functional_scenario_run)").bind(project).bind(id).bind(snapshot["revision"].as_i64().ok_or(AssetError::Storage)? as i32).bind(&snapshot).bind(&v["steps"]).bind(text(v,"environment")).bind(outcome).bind(text(v,"notes")).bind(user).fetch_one(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(row)
    }
    async fn scenario_runs(&self, project: &str, id: &str) -> AssetResult<Value> {
        let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(r) FROM ms_functional_scenario_run r WHERE project_id=$1 AND scenario_id=$2 ORDER BY executed_at DESC LIMIT 100").bind(project).bind(id).fetch_all(&self.0).await.map_err(db)?;
        Ok(json!({"items":rows}))
    }
    async fn save_inventory(
        &self,
        project: &str,
        user: &str,
        service: &str,
        v: &Value,
    ) -> AssetResult<Value> {
        service_exists(&self.0, project, service).await?;
        let (repo, branch, sha, state) =
            (text(v, "repositoryId"), text(v, "branch"), text(v, "commitSha"), text(v, "state"));
        if !valid_sha(sha) || !matches!(state, "PARTIAL" | "REVIEWED" | "FAILED") {
            return Err(AssetError::Invalid(
                "inventory requires a commit SHA and valid state".into(),
            ));
        }
        let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_test_service s,jsonb_array_elements(s.repositories) r WHERE s.id=$1 AND s.project_id=$2 AND r->>'id'=$3 AND r->>'branch'=$4)").bind(service).bind(project).bind(repo).bind(branch).fetch_one(&self.0).await.map_err(db)?;
        if !bound {
            return Err(AssetError::Invalid(
                "inventory repository and branch must belong to the service".into(),
            ));
        }
        let endpoints = v["endpoints"]
            .as_array()
            .ok_or_else(|| AssetError::Invalid("endpoints must be an array".into()))?;
        let limits = v["limitations"]
            .as_array()
            .ok_or_else(|| AssetError::Invalid("limitations must be an array".into()))?;
        if endpoints.len() > 10000 || limits.len() > 200 || !v["scannedPaths"].is_array() {
            return Err(AssetError::Invalid("invalid inventory size or scanned paths".into()));
        }
        for e in endpoints {
            bounded(text(e, "path"), 2000, true)?;
            bounded(text(e, "file"), 2000, true)?;
            if !matches!(
                text(e, "method"),
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "ANY"
            ) || e["line"].as_u64().is_none_or(|n| n == 0)
                || text(e, "file").split('/').any(|s| s == "..")
            {
                return Err(AssetError::Invalid(
                    "endpoint needs an HTTP method and source location".into(),
                ));
            }
        }
        for e in endpoints {
            if let Some(id) = e["apiDefinitionId"].as_str().filter(|s| !s.is_empty()) {
                let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ms_api_definition WHERE project_id=$1 AND id=$2 AND NOT deleted)").bind(project).bind(id).fetch_one(&self.0).await.map_err(db)?;
                if !allowed {
                    return Err(AssetError::Invalid(
                        "API definition is outside the project".into(),
                    ));
                }
            }
            if let Some(ids) = e["apiCaseIds"].as_array() {
                let ids: Vec<String> =
                    ids.iter().filter_map(Value::as_str).map(str::to_string).collect();
                let count: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM ms_api_case WHERE project_id=$1 AND id=ANY($2)",
                )
                .bind(project)
                .bind(&ids)
                .fetch_one(&self.0)
                .await
                .map_err(db)?;
                if count != ids.iter().collect::<std::collections::BTreeSet<_>>().len() as i64 {
                    return Err(AssetError::Invalid("API case is outside the project".into()));
                }
            }
        }
        let base = if let Some(id) = v["baseInventoryId"].as_str() {
            Some(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(i) FROM ms_test_inventory i WHERE project_id=$1 AND service_id=$2 AND id=$3 AND repository_id=$4 AND branch=$5 AND commit_sha=$6").bind(project).bind(service).bind(id).bind(repo).bind(branch).bind(sha).fetch_optional(&self.0).await.map_err(db)?.ok_or_else(||AssetError::Invalid("inventory base does not match".into()))?)
        } else {
            None
        };
        let ids: Vec<String> = base
            .as_ref()
            .and_then(|v| v["source_document_ids"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let blobs = base.as_ref().map(|v| v["file_blobs"].clone()).unwrap_or(json!({}));
        if state == "REVIEWED" && (endpoints.is_empty() || !limits.is_empty()) {
            return Err(AssetError::Invalid(
                "resolve inventory gaps before marking reviewed".into(),
            ));
        }
        sqlx::query_scalar("INSERT INTO ms_test_inventory(project_id,service_id,repository_id,branch,commit_sha,state,endpoints,limitations,scanned_paths,reviewed_by,created_by,source_document_ids,file_blobs) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING to_jsonb(ms_test_inventory)").bind(project).bind(service).bind(repo).bind(branch).bind(sha).bind(state).bind(&v["endpoints"]).bind(&v["limitations"]).bind(&v["scannedPaths"]).bind(if state=="REVIEWED" {Some(user)}else{None}).bind(user).bind(&ids).bind(blobs).fetch_one(&self.0).await.map_err(db)
    }
}

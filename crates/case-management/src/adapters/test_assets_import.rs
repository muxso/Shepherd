use super::test_assets_pg::db;
use crate::domain::test_assets::*;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
pub fn desired(r: &SourceRecord) -> AssetResult<Value> {
    let name = text(&r.data, "title");
    bounded(name, 16000, true)?;
    let serial = text(&r.data, "serial");
    let name = if !serial.is_empty() && !name.starts_with(serial) {
        format!("{serial} {name}")
    } else {
        name.to_string()
    };
    if r.kind == "testcase" {
        let steps = r.data.get("steps").cloned().unwrap_or(json!([]));
        let parsed: Vec<crate::domain::CaseStep> = serde_json::from_value(steps.clone())
            .map_err(|_| AssetError::Invalid("invalid source case steps".into()))?;
        if parsed.len() > 1000 {
            return Err(AssetError::Invalid("source case has too many steps".into()));
        }
        Ok(
            json!({"name":name,"priority":if text(&r.data,"priority").is_empty(){"P2"}else{text(&r.data,"priority")},"steps":steps,"precondition":text(&r.data,"precondition")}),
        )
    } else if r.kind == "requirement" {
        let serial = text(&r.data, "serial");
        let title = if serial.is_empty() || name.starts_with(serial) {
            name.to_string()
        } else {
            format!("{serial} {name}")
        };
        Ok(json!({"title":title,"description":text(&r.data,"description")}))
    } else {
        Err(AssetError::Invalid("only requirements and functional cases can be imported".into()))
    }
}
async fn current<'e, E>(executor: E, project: &str) -> AssetResult<Vec<Value>>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar("SELECT jsonb_build_object('kind','testcase','nativeId',c.id,'sourceId',COALESCE(o.source_id,to_jsonb(c)->>'external_source_id'),'externalId',COALESCE(o.external_id,to_jsonb(c)->>'external_id'),'keepLocal',o.keep_local,'baseline',o.native_snapshot,'fields',jsonb_build_object('name',c.name,'priority',c.priority,'steps',c.steps,'precondition',COALESCE(c.custom_fields->>'前置条件','')),'deleted',c.deleted,'sourceFields',o.source_payload) FROM ms_functional_case c LEFT JOIN ms_test_asset_origin o ON o.kind='testcase' AND o.native_id=c.id WHERE c.project_id=$1 UNION ALL SELECT jsonb_build_object('kind','requirement','nativeId',r.id,'sourceId',COALESCE(o.source_id,to_jsonb(r)->>'external_source_id'),'externalId',COALESCE(o.external_id,to_jsonb(r)->>'external_id'),'keepLocal',o.keep_local,'baseline',o.native_snapshot,'fields',jsonb_build_object('title',r.title,'description',COALESCE(v.description,'')),'deleted',r.deleted,'archived',r.status='ARCHIVED','sourceFields',o.source_payload) FROM ms_requirement r LEFT JOIN LATERAL(SELECT description FROM ms_requirement_version WHERE requirement_id=r.id ORDER BY version DESC LIMIT 1) v ON true LEFT JOIN ms_test_asset_origin o ON o.kind='requirement' AND o.native_id=r.id WHERE r.project_id=$1").bind(project.to_owned()).fetch_all(executor).await.map_err(db)
}
pub async fn preview(
    pool: &PgPool,
    project: &str,
    user: &str,
    snapshot: &SourceSnapshot,
) -> AssetResult<Value> {
    bounded(&snapshot.source_id, 255, true)?;
    bounded(&snapshot.revision, 255, true)?;
    if snapshot.records.is_empty() || snapshot.records.len() > 100000 {
        return Err(AssetError::Invalid("source snapshot must contain 1..100000 assets".into()));
    }
    let existing = current(pool, project).await?;
    let lookup: BTreeMap<_, _> = existing
        .iter()
        .filter(|v| text(v, "sourceId") == snapshot.source_id)
        .map(|v| ((text(v, "kind"), text(v, "externalId")), v))
        .collect();
    if lookup.len() != existing.iter().filter(|v| text(v, "sourceId") == snapshot.source_id).count()
    {
        return Err(AssetError::Conflict(
            "duplicate legacy source identities require reconciliation".into(),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    let mut counts = BTreeMap::<String, usize>::new();
    for r in &snapshot.records {
        bounded(&r.external_id, 1000, true)?;
        if !seen.insert((r.kind.as_str(), r.external_id.as_str())) {
            return Err(AssetError::Invalid("duplicate source asset".into()));
        }
        let target = desired(r)?;
        let previous = lookup.get(&(r.kind.as_str(), r.external_id.as_str()));
        let action = match previous {
            None => "CREATE",
            Some(v) if v["deleted"] == true || (v["archived"] == true && v["fields"] != target) => {
                "CONFLICT"
            }
            Some(v) if v["fields"] == target => {
                if v["baseline"].is_null() {
                    "ADOPT"
                } else {
                    "UNCHANGED"
                }
            }
            Some(v)
                if v["keepLocal"] != true
                    && !v["baseline"].is_null()
                    && v["fields"] == v["baseline"] =>
            {
                "UPDATE"
            }
            _ => "CONFLICT",
        };
        *counts.entry(action.into()).or_default() += 1;
        rows.push(json!({"kind":r.kind,"externalId":r.external_id,"nativeId":previous.map(|v|v["nativeId"].clone()),"action":action,"title":text(&r.data,"title"),"current":previous.map(|v|v["fields"].clone()),"target":target,"deleted":previous.is_some_and(|v|v["deleted"]==true),"archived":previous.is_some_and(|v|v["archived"]==true)}));
    }
    let missing:Vec<Value>=lookup.iter().filter(|(k,_)|!seen.contains(k)).map(|(_,v)|json!({"kind":v["kind"],"externalId":v["externalId"],"nativeId":v["nativeId"],"action":"SOURCE_MISSING","title":v["fields"]["name"].as_str().or_else(||v["fields"]["title"].as_str()).unwrap_or("")})).collect();
    counts.insert("SOURCE_MISSING".into(), missing.len());
    rows.extend(missing);
    let preview = json!({"counts":counts,"items":rows,"manualAssets":existing.iter().filter(|v|v["sourceId"].is_null()).count(),"sourceId":snapshot.source_id,"sourceRevision":snapshot.revision});
    let records = serde_json::to_value(&snapshot.records).map_err(|_| AssetError::Storage)?;
    let mut result:Value = sqlx::query_scalar("INSERT INTO ms_test_import(project_id,source_id,source_revision,records,preview,created_by) VALUES($1,$2,$3,$4,$5,$6) RETURNING to_jsonb(ms_test_import)-'records'").bind(project).bind(&snapshot.source_id).bind(&snapshot.revision).bind(records).bind(preview).bind(user).fetch_one(pool).await.map_err(db)?;
    if let Some(rows) = result["preview"]["items"].as_array_mut() {
        for row in rows {
            if let Some(row) = row.as_object_mut() {
                row.remove("current");
                row.remove("target");
            }
        }
    }
    Ok(result)
}
async fn module(
    tx: &mut Transaction<'_, Postgres>,
    project: &str,
    data: &Value,
) -> AssetResult<String> {
    let mut parent: Option<String> = None;
    for name in data
        .get("modulePath")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(30)
    {
        bounded(name, 1000, true)?;
        let id:Option<String>=sqlx::query_scalar("SELECT id FROM ms_api_module WHERE project_id=$1 AND parent_id IS NOT DISTINCT FROM $2 AND name=$3 AND NOT deleted ORDER BY id LIMIT 1").bind(project).bind(&parent).bind(name).fetch_optional(&mut **tx).await.map_err(db)?;
        parent=Some(match id{Some(id)=>id,None=>sqlx::query_scalar("INSERT INTO ms_api_module(project_id,parent_id,name) VALUES($1,$2,$3) RETURNING id").bind(project).bind(&parent).bind(name).fetch_one(&mut **tx).await.map_err(db)?});
    }
    Ok(parent.unwrap_or_default())
}
pub async fn apply(
    pool: &PgPool,
    project: &str,
    user: &str,
    id: &str,
    revision: &str,
    resolutions: &Value,
) -> AssetResult<Value> {
    let mut tx = pool.begin().await.map_err(db)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("test-import:{project}"))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let batch: Value = sqlx::query_scalar(
        "SELECT to_jsonb(b) FROM ms_test_import b WHERE project_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(project)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db)?
    .ok_or(AssetError::NotFound)?;
    if text(&batch, "status") == "APPLIED" {
        return Ok(batch["result"].clone());
    }
    if text(&batch, "created_by") != user {
        return Err(AssetError::Forbidden);
    }
    if text(&batch, "source_revision") != revision {
        return Err(AssetError::Conflict("source changed; refresh the import preview".into()));
    }
    let fresh: bool = sqlx::query_scalar(
        "SELECT created_at>now()-interval '1 hour' FROM ms_test_import WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db)?;
    if !fresh {
        return Err(AssetError::Conflict("import preview expired".into()));
    }
    sqlx::query("SELECT id FROM ms_functional_case WHERE project_id=$1 FOR UPDATE")
        .bind(project)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    sqlx::query("SELECT id FROM ms_requirement WHERE project_id=$1 FOR UPDATE")
        .bind(project)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    let live = current(&mut *tx, project).await?;
    let by_id: BTreeMap<_, _> =
        live.iter().map(|v| ((text(v, "kind"), text(v, "nativeId")), v)).collect();
    let source = text(&batch, "source_id");
    let records: Vec<SourceRecord> =
        serde_json::from_value(batch["records"].clone()).map_err(|_| AssetError::Storage)?;
    let decisions: BTreeMap<_, _> = batch["preview"]["items"]
        .as_array()
        .ok_or(AssetError::Storage)?
        .iter()
        .map(|r| ((text(r, "kind"), text(r, "externalId")), r))
        .collect();
    for row in decisions
        .values()
        .filter(|r| !r["nativeId"].is_null() && text(r, "action") != "SOURCE_MISSING")
    {
        let live = by_id
            .get(&(text(row, "kind"), text(row, "nativeId")))
            .ok_or_else(|| AssetError::Conflict("native asset changed; refresh preview".into()))?;
        if live["fields"] != row["current"] || live["deleted"] != row["deleted"] {
            return Err(AssetError::Conflict("native asset changed; refresh preview".into()));
        }
    }
    let mut imported = 0usize;
    let mut conflicts = 0usize;
    let mut unchanged = 0usize;
    for r in &records {
        let row =
            decisions.get(&(r.kind.as_str(), r.external_id.as_str())).ok_or(AssetError::Storage)?;
        let original_action = text(row, "action");
        let resolution_key = format!("{}:{}", r.kind, r.external_id);
        let resolution = resolutions.get(&resolution_key).and_then(Value::as_str).unwrap_or("");
        let action = if original_action == "CONFLICT" {
            match resolution {
                "keep-local" => "ADOPT_LOCAL",
                "use-source" if row["deleted"] != true && row["archived"] != true => "UPDATE",
                "" => {
                    conflicts += 1;
                    continue;
                }
                _ => return Err(AssetError::Invalid("invalid conflict decision".into())),
            }
        } else {
            original_action
        };
        if action == "UNCHANGED" {
            sqlx::query("UPDATE ms_test_asset_origin SET source_payload=$5,source_revision=$6,source_present=true,synced_at=now() WHERE project_id=$1 AND source_id=$2 AND kind=$3 AND external_id=$4").bind(project).bind(source).bind(&r.kind).bind(&r.external_id).bind(&r.data).bind(revision).execute(&mut *tx).await.map_err(db)?;
            unchanged += 1;
            continue;
        }
        let target = if action == "ADOPT_LOCAL" { &row["current"] } else { &row["target"] };
        let native_id = if action == "CREATE" {
            if r.kind == "testcase" {
                let folder = module(&mut tx, project, &r.data).await?;
                let custom = json!({"来源":"云效","云效编号":text(&r.data,"serial"),"云效用例库":text(&r.data,"repo"),"云效目录":text(&r.data,"directory"),"云效负责人":text(&r.data,"owner"),"前置条件":text(target,"precondition"),"来源链接":text(&r.data,"url"),"来源创建时间":text(&r.data,"createdAt")});
                sqlx::query_scalar::<_,String>("INSERT INTO ms_functional_case(project_id,num,name,module,priority,status,custom_fields,steps,created_by) VALUES($1,(SELECT COALESCE(max(num)+1,100001) FROM ms_functional_case WHERE project_id=$1),$2,$3,$4,'PREPARED',$5,$6,$7) RETURNING id").bind(project).bind(text(target,"name")).bind(folder).bind(text(target,"priority")).bind(custom).bind(&target["steps"]).bind(text(&r.data,"creator")).fetch_one(&mut *tx).await.map_err(db)?
            } else {
                let id:String=sqlx::query_scalar("INSERT INTO ms_requirement(project_id,title,created_by,custom_fields) VALUES($1,$2,$3,$4) RETURNING id").bind(project).bind(text(target,"title")).bind(user).bind(json!({"来源":"云效","云效编号":text(&r.data,"serial"),"来源状态":text(&r.data,"status"),"来源链接":text(&r.data,"url")})).fetch_one(&mut *tx).await.map_err(db)?;
                sqlx::query("INSERT INTO ms_requirement_version(requirement_id,version,description) VALUES($1,1,$2)").bind(&id).bind(text(target,"description")).execute(&mut *tx).await.map_err(db)?;
                sqlx::query("INSERT INTO ms_requirement_stage(requirement_id,stage,status) SELECT $1,s,CASE WHEN s='CREATED' THEN 'DONE' ELSE 'PENDING' END FROM unnest(ARRAY['CREATED','AUDIT','REVIEW','DEV','TEST','ACCEPTANCE','DELIVERY']) s").bind(&id).execute(&mut *tx).await.map_err(db)?;
                id
            }
        } else {
            let id = text(row, "nativeId").to_owned();
            if action == "UPDATE" {
                if r.kind == "testcase" {
                    sqlx::query("UPDATE ms_functional_case SET name=$3,priority=$4,steps=$5,custom_fields=custom_fields||jsonb_build_object('前置条件',$6::text),updated_at=now() WHERE id=$1 AND project_id=$2 AND NOT deleted").bind(&id).bind(project).bind(text(target,"name")).bind(text(target,"priority")).bind(&target["steps"]).bind(text(target,"precondition")).execute(&mut *tx).await.map_err(db)?;
                    sqlx::query("INSERT INTO ms_functional_case_change(case_id,field,old_value,new_value,actor) VALUES($1,'source-sync',$2,$3,$4)").bind(&id).bind(row["current"].to_string()).bind(target.to_string()).bind(user).execute(&mut *tx).await.map_err(db)?;
                } else {
                    sqlx::query("UPDATE ms_requirement SET title=$3,updated_at=now() WHERE id=$1 AND project_id=$2 AND NOT deleted").bind(&id).bind(project).bind(text(target,"title")).execute(&mut *tx).await.map_err(db)?;
                    sqlx::query("INSERT INTO ms_requirement_version(requirement_id,version,description,acceptance_criteria) SELECT $1,version+1,$2,acceptance_criteria FROM ms_requirement_version WHERE requirement_id=$1 ORDER BY version DESC LIMIT 1").bind(&id).bind(text(target,"description")).execute(&mut *tx).await.map_err(db)?;
                }
            }
            id
        };
        sqlx::query("INSERT INTO ms_test_asset_origin(project_id,provider,source_id,kind,external_id,native_id,source_payload,native_snapshot,source_revision,keep_local) VALUES($1,'yunxiao',$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(project_id,provider,source_id,kind,external_id) DO UPDATE SET source_payload=EXCLUDED.source_payload,native_snapshot=EXCLUDED.native_snapshot,source_revision=EXCLUDED.source_revision,keep_local=EXCLUDED.keep_local,source_present=true,synced_at=now()").bind(project).bind(source).bind(&r.kind).bind(&r.external_id).bind(native_id).bind(&r.data).bind(target).bind(revision).bind(action == "ADOPT_LOCAL").execute(&mut *tx).await.map_err(db)?;
        imported += 1;
    }
    for row in decisions.values().filter(|r| text(r, "action") == "SOURCE_MISSING") {
        if let Some(live) = by_id.get(&(text(row, "kind"), text(row, "nativeId"))) {
            sqlx::query("INSERT INTO ms_test_asset_origin(project_id,provider,source_id,kind,external_id,native_id,source_payload,native_snapshot,source_revision,keep_local,source_present) VALUES($1,'yunxiao',$2,$3,$4,$5,'{}',$6,$7,true,false) ON CONFLICT(project_id,provider,source_id,kind,external_id) DO UPDATE SET source_present=false,synced_at=now()").bind(project).bind(source).bind(text(row,"kind")).bind(text(row,"externalId")).bind(text(row,"nativeId")).bind(&live["fields"]).bind(revision).execute(&mut *tx).await.map_err(db)?;
        }
    }
    let external: Vec<String> =
        records.iter().map(|r| format!("{}:{}", r.kind, r.external_id)).collect();
    sqlx::query("UPDATE ms_test_asset_origin SET source_present=false WHERE project_id=$1 AND source_id=$2 AND NOT(kind||':'||external_id=ANY($3))").bind(project).bind(source).bind(&external).execute(&mut *tx).await.map_err(db)?;
    let result = json!({"imported":imported,"unchanged":unchanged,"conflicts":conflicts,"sourceMissing":batch["preview"]["counts"]["SOURCE_MISSING"],"sourceRevision":revision});
    sqlx::query(
        "UPDATE ms_test_import SET status='APPLIED',applied_at=now(),result=$2 WHERE id=$1",
    )
    .bind(id)
    .bind(&result)
    .execute(&mut *tx)
    .await
    .map_err(db)?;
    tx.commit().await.map_err(db)?;
    Ok(result)
}

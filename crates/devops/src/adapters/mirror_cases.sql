WITH src AS (
 SELECT e.*,row_number() OVER(ORDER BY external_id) AS n FROM ms_devops_entity e WHERE source_id=$1 AND active AND kind='testcase'
), base AS (SELECT COALESCE(MAX(num),100000) AS maxnum FROM ms_functional_case WHERE project_id=$2)
INSERT INTO ms_functional_case(id,project_id,num,name,module,priority,status,tags,custom_fields,steps,created_by,created_at,updated_at,external_source_id,external_id)
SELECT 'yxcase-'||md5($1||':'||external_id),$2,base.maxnum+n,
 COALESCE(NULLIF(payload->>'serial',''),external_id)||' '||COALESCE(payload->>'title',''),
 CASE WHEN jsonb_array_length(COALESCE(payload->'modulePath','[]'))>0 THEN 'yxmodule-'||md5($1||':'||(payload->'modulePath')::text) ELSE '' END,
 COALESCE(NULLIF(payload->>'priority',''),'P2'),'PREPARED',COALESCE(payload->'labels','[]'),
 jsonb_build_object('来源','云效','云效编号',COALESCE(payload->>'serial',''),'云效用例库',COALESCE(payload->>'repo',''),'云效目录',COALESCE(payload->>'directory',''),'云效负责人',COALESCE(payload->>'owner',''),'前置条件',COALESCE(payload->>'precondition','')),
 COALESCE(payload->'steps','[]'),NULLIF(payload->>'creator',''),COALESCE(NULLIF(payload->>'createdAt','')::timestamptz,now()),COALESCE(NULLIF(payload->>'updatedAt','')::timestamptz,now()),$1,external_id
FROM src CROSS JOIN base
ON CONFLICT(id) DO UPDATE SET name=EXCLUDED.name,module=EXCLUDED.module,priority=EXCLUDED.priority,tags=EXCLUDED.tags,
 custom_fields=ms_functional_case.custom_fields||EXCLUDED.custom_fields,steps=EXCLUDED.steps,created_by=EXCLUDED.created_by,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at,deleted=false
WHERE ms_functional_case.external_source_id=$1

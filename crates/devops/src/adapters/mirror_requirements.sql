INSERT INTO ms_requirement(id,project_id,title,status,priority,due_date,created_at,updated_at,custom_fields,module_id,external_source_id,external_id,external_content_hash)
SELECT 'yxreq-'||md5($1||':'||external_id),$2,
 left(COALESCE(NULLIF(payload->>'serial',''),external_id)||' '||COALESCE(payload->>'title',''),255),
 CASE WHEN payload->>'status' IN ('已发布','已完成','已验收','完成') THEN 'DELIVERED'
 WHEN payload->>'status' IN ('已取消','已关闭') THEN 'ARCHIVED'
 WHEN payload->>'status' IN ('开发中','进行中','测试中','待测试','待提测','待发布','待上线','验收中') THEN 'BASELINED' ELSE 'DRAFT' END,
 CASE payload->>'priority' WHEN '紧急' THEN 'P0' WHEN '高' THEN 'P1' WHEN '低' THEN 'P3' ELSE 'P2' END,
 NULLIF(payload->>'due','')::date,
 COALESCE(NULLIF(payload->>'createdAt','')::timestamptz,now()),COALESCE(NULLIF(payload->>'updatedAt','')::timestamptz,now()),
 jsonb_build_object('来源','云效','云效编号',COALESCE(payload->>'serial',''),'云效状态',COALESCE(payload->>'status',''),'云效负责人',COALESCE(payload->>'owner',''),'云效创建人',COALESCE(payload->'raw'->'creator'->>'name',''),'云效优先级',COALESCE(payload->>'priority',''),'云效迭代',COALESCE(payload->>'sprint','')),
 'yxmodule-'||md5($1||':'||'["云效需求"]'::jsonb::text),$1,external_id,md5(COALESCE(payload->'raw'->>'description',''))
FROM ms_devops_entity WHERE source_id=$1 AND kind='requirement' AND active
ON CONFLICT(id) DO UPDATE SET title=EXCLUDED.title,status=EXCLUDED.status,priority=EXCLUDED.priority,due_date=EXCLUDED.due_date,
 updated_at=EXCLUDED.updated_at,custom_fields=ms_requirement.custom_fields||EXCLUDED.custom_fields,deleted=false,
 baseline_version=CASE WHEN ms_requirement.external_content_hash IS DISTINCT FROM EXCLUDED.external_content_hash THEN ms_requirement.baseline_version+1 ELSE ms_requirement.baseline_version END,
 external_content_hash=EXCLUDED.external_content_hash
WHERE ms_requirement.external_source_id=$1

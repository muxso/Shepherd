WITH requirements AS (
 SELECT r.*,CASE WHEN r.status='DELIVERED' THEN 7 WHEN r.custom_fields->>'云效状态' IN ('待发布','待上线','验收中') THEN 6
 WHEN r.custom_fields->>'云效状态' IN ('测试中','待测试','待提测') THEN 5 WHEN r.custom_fields->>'云效状态' IN ('开发中','进行中') THEN 4 ELSE 2 END AS phase
 FROM ms_requirement r WHERE r.external_source_id=$1 AND NOT deleted
), stages AS (SELECT * FROM (VALUES ('CREATED',1),('AUDIT',2),('REVIEW',3),('DEV',4),('TEST',5),('ACCEPTANCE',6),('DELIVERY',7)) t(stage,n))
INSERT INTO ms_requirement_stage(requirement_id,stage,status,planned_end,started_at)
SELECT r.id,s.stage,CASE WHEN r.status='DELIVERED' OR s.n<r.phase THEN 'DONE' WHEN s.n=r.phase AND r.status='BASELINED' THEN 'IN_PROGRESS' ELSE 'PENDING' END,
 CASE WHEN s.stage='DELIVERY' THEN r.due_date ELSE NULL END,CASE WHEN s.stage='CREATED' THEN r.created_at ELSE NULL END
FROM requirements r CROSS JOIN stages s
ON CONFLICT(requirement_id,stage) DO UPDATE SET status=EXCLUDED.status,planned_end=EXCLUDED.planned_end

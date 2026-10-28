WITH req AS (
 SELECT r.id,r.title,r.status,e.payload->'raw' AS raw,
 NOT EXISTS(SELECT 1 FROM ms_task_decomposition d JOIN ms_task t ON t.decomposition_id=d.id WHERE d.requirement_id=r.id) AS no_tasks
 FROM ms_requirement r LEFT JOIN ms_devops_entity e
 ON e.source_id=r.external_source_id AND e.external_id=r.external_id AND e.kind='requirement' AND e.active
 WHERE r.project_id=$1 AND NOT r.deleted AND ($2='' OR r.id=$2)
), registrations AS (
 SELECT id AS requirement_id,raw->'assignedTo'->>'id' AS person_id,raw->'assignedTo'->>'name' AS person_name,'负责人' AS role FROM req
 UNION ALL
 SELECT r.id,p->>'id',p->>'name','参与人' FROM req r
 CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(raw->'participants')='array' THEN raw->'participants' ELSE '[]' END) p
 UNION ALL
 SELECT r.id,COALESCE(NULLIF(v->>'identifier',''),'name:'||(v->>'displayValue')),v->>'displayValue',f->>'fieldName'
 FROM req r CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(raw->'customFieldValues')='array' THEN raw->'customFieldValues' ELSE '[]' END) f
 CROSS JOIN LATERAL jsonb_array_elements(CASE WHEN jsonb_typeof(f->'values')='array' THEN f->'values' ELSE '[]' END) v
 WHERE f->>'fieldName' IN ('开发者','测试人员','参与人','参与者','协同人','协作人')
), roster AS (
 SELECT requirement_id,COALESCE(NULLIF(person_id,''),'name:'||person_name) AS person_id,
 max(person_name) AS person_name,array_agg(DISTINCT role ORDER BY role) AS roles
 FROM registrations WHERE NULLIF(trim(person_name),'') IS NOT NULL
 GROUP BY requirement_id,COALESCE(NULLIF(person_id,''),'name:'||person_name)
), assigned AS (
 SELECT r.id AS requirement_id,t.*,
 COALESCE(NULLIF(u.name,''),t.assignee) AS assigned_name,
 EXISTS(SELECT 1 FROM ms_delivery_attempt a WHERE a.decomposition_id=t.decomposition_id AND a.task_id=t.id AND a.status='DELIVERED') AS ai
 FROM req r JOIN ms_task_decomposition d ON d.requirement_id=r.id JOIN ms_task t ON t.decomposition_id=d.id
 LEFT JOIN ms_user u ON u.id=t.assignee
 WHERE NULLIF(trim(t.assignee),'') IS NOT NULL AND t.assignee_kind IN ('HUMAN','')
), task_people AS (
 SELECT a.*,
 COALESCE((SELECT p.person_id FROM roster p WHERE p.requirement_id=a.requirement_id AND p.person_id=a.assignee),
          (SELECT min(p.person_id) FROM roster p WHERE p.requirement_id=a.requirement_id AND p.person_name=a.assigned_name HAVING count(*)=1),a.assignee) AS person_id
 FROM assigned a
), members AS (
 SELECT requirement_id,person_id,person_name,unnest(roles) AS role FROM roster
 UNION ALL
 SELECT requirement_id,person_id,assigned_name,'任务负责人' FROM task_people
), people AS (
 SELECT requirement_id,person_id,max(person_name) AS person_name,array_agg(DISTINCT role ORDER BY role) AS roles
 FROM members GROUP BY requirement_id,person_id
), task_stats AS (
 SELECT requirement_id,person_id,
 count(*) FILTER(WHERE status='VERIFIED' AND ai) AS ai_tasks,
 count(*) FILTER(WHERE status='VERIFIED' AND NOT ai) AS human_tasks,
 COALESCE(sum(points) FILTER(WHERE status='VERIFIED'),0)::bigint AS points
 FROM task_people GROUP BY requirement_id,person_id
)
SELECT r.id AS requirement_id,r.title,r.status,
 COALESCE(p.person_id,'') AS person_id,COALESCE(p.person_name,'未登记') AS person_name,COALESCE(p.roles,'{}'::text[]) AS roles,
 r.status='DELIVERED' AS delivered,
 CASE WHEN r.status='DELIVERED' AND r.no_tasks AND p.person_id IS NOT NULL
 THEN 1.0/count(p.person_id) OVER(PARTITION BY r.id) ELSE 0 END::double precision AS requirement_share,
 COALESCE(s.ai_tasks,0)::bigint AS ai_tasks,COALESCE(s.human_tasks,0)::bigint AS human_tasks,COALESCE(s.points,0)::bigint AS points
FROM req r LEFT JOIN people p ON p.requirement_id=r.id
LEFT JOIN task_stats s ON s.requirement_id=p.requirement_id AND s.person_id=p.person_id
ORDER BY r.id,p.person_id

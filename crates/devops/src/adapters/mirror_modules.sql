WITH full_paths AS (
 SELECT '["云效需求"]'::jsonb AS path
 UNION
 SELECT payload->'modulePath' FROM ms_devops_entity WHERE source_id=$1 AND active AND kind='testcase' AND jsonb_typeof(payload->'modulePath')='array'
), paths AS (
 SELECT DISTINCT (SELECT jsonb_agg(value ORDER BY ordinality) FROM jsonb_array_elements(f.path) WITH ORDINALITY WHERE ordinality<=n) AS path
 FROM full_paths f CROSS JOIN LATERAL generate_series(1,jsonb_array_length(f.path)) n
)
INSERT INTO ms_api_module(id,project_id,parent_id,name)
SELECT 'yxmodule-'||md5($1||':'||path::text),$2,
 CASE WHEN jsonb_array_length(path)>1 THEN 'yxmodule-'||md5($1||':'||(path-(jsonb_array_length(path)-1))::text) ELSE NULL END,
 path->>(jsonb_array_length(path)-1)
FROM paths
ON CONFLICT(id) DO UPDATE SET name=EXCLUDED.name,parent_id=EXCLUDED.parent_id,deleted=false

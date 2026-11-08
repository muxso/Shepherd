WITH bugs AS MATERIALIZED (
 SELECT external_id,payload FROM ms_devops_statistics_entity WHERE source_id=$1 AND kind='bug' AND active
), requirements AS (
 SELECT external_id AS reference FROM ms_devops_statistics_entity WHERE source_id=$1 AND kind='requirement' AND active
 UNION SELECT payload->>'serial' FROM ms_devops_statistics_entity WHERE source_id=$1 AND kind='requirement' AND active
), refs AS (
 SELECT external_id,jsonb_array_elements_text(COALESCE(payload->'requirementIds','[]')||to_jsonb(array_remove(ARRAY[NULLIF(payload->>'parentId',''),NULLIF(payload->>'parentSerial','')],NULL))) AS reference FROM bugs
), linked AS (
 SELECT DISTINCT external_id FROM refs JOIN requirements USING(reference)
)
SELECT jsonb_build_object('total',(SELECT count(*) FROM bugs),
 'new', (SELECT count(*) FROM bugs WHERE (NULLIF(payload->>'createdAt','')::timestamptz AT TIME ZONE 'Asia/Shanghai')::date BETWEEN (now() AT TIME ZONE 'Asia/Shanghai')::date-$2::int+1 AND (now() AT TIME ZONE 'Asia/Shanghai')::date),
 'linked',(SELECT count(*) FROM linked),'unlinked',(SELECT count(*) FROM bugs)-(SELECT count(*) FROM linked),
 'statuses',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY count DESC,status) FROM (SELECT COALESCE(NULLIF(payload->>'status',''),'未记录') AS status,count(*) FROM bugs GROUP BY 1) s),'[]'),
 'reporters',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY count DESC,name) FROM (SELECT COALESCE(NULLIF(payload->>'creator',''),NULLIF(payload->'raw'->'creator'->>'name',''),'待归属') AS name,count(*) FROM bugs GROUP BY 1) p),'[]')
)

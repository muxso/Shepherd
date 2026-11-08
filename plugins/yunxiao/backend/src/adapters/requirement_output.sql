WITH bugs AS MATERIALIZED (
 SELECT external_id,payload FROM ms_devops_statistics_resolved WHERE source_id=$1 AND kind='bug' AND active
 AND (payload->>'parentId' IN ($2,$3) OR payload->>'parentSerial'=$3 OR COALESCE(payload->'requirementIds','[]') ? $2 OR COALESCE(payload->'requirementIds','[]') ? $3)
 AND ($4::int IS NULL OR (NULLIF(payload->>'createdAt','')::timestamptz AT TIME ZONE 'Asia/Shanghai')::date BETWEEN (now() AT TIME ZONE 'Asia/Shanghai')::date-$4+1 AND (now() AT TIME ZONE 'Asia/Shanghai')::date)
), commits AS MATERIALIZED (
 SELECT external_id,payload FROM ms_devops_statistics_resolved WHERE source_id=$1 AND kind='commit' AND active
 AND (COALESCE(payload->'requirementIds','[]') ? $2 OR COALESCE(payload->'requirementIds','[]') ? $3)
 AND ($4::int IS NULL OR (NULLIF(payload->>'authoredAt','')::timestamptz AT TIME ZONE 'Asia/Shanghai')::date BETWEEN (now() AT TIME ZONE 'Asia/Shanghai')::date-$4+1 AND (now() AT TIME ZONE 'Asia/Shanghai')::date)
), testers AS (
 SELECT COALESCE(NULLIF(payload->>'creator',''),NULLIF(payload->'raw'->'creator'->>'name',''),'待归属') AS name,count(*) AS bugs FROM bugs GROUP BY 1
), developers AS (
 SELECT COALESCE(NULLIF(payload->>'author',''),'待归属') AS name,count(*) AS commits FROM commits GROUP BY 1
), repositories AS (
 SELECT COALESCE(NULLIF(payload->>'repoId',''),NULLIF(payload->>'repo',''),'待归属') AS id,
 COALESCE((array_agg(NULLIF(payload->>'repo','') ORDER BY (NULLIF(payload->>'authoredAt','')::timestamptz) DESC) FILTER(WHERE NULLIF(payload->>'repo','') IS NOT NULL))[1],'未记录仓库') AS name,count(*) AS commits,
 sum((payload->>'additions')::bigint) AS additions,sum((payload->>'deletions')::bigint) AS deletions,
 count(*) FILTER(WHERE payload->>'additions' IS NULL OR payload->>'deletions' IS NULL) AS "unknownStats",
 array_agg(DISTINCT COALESCE(NULLIF(payload->>'author',''),'待归属')) AS authors
 FROM commits GROUP BY 1
)
SELECT jsonb_build_object(
 'bugTotal',(SELECT count(*) FROM bugs), 'commitTotal',(SELECT count(*) FROM commits),
 'testers',COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY bugs DESC,name) FROM testers t),'[]'),
 'developers',COALESCE((SELECT jsonb_agg(to_jsonb(d) ORDER BY commits DESC,name) FROM developers d),'[]'),
 'repositories',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY commits DESC,id) FROM repositories r),'[]'),
 'bugStates',COALESCE((SELECT jsonb_agg(to_jsonb(s) ORDER BY count DESC,status) FROM (SELECT COALESCE(NULLIF(payload->>'status',''),'未记录') AS status,count(*) FROM bugs GROUP BY 1) s),'[]'),
 'bugs',COALESCE((SELECT jsonb_agg(p.row ORDER BY p.created DESC,p.id) FROM (
   SELECT external_id AS id,payload->>'createdAt' AS created,(payload-'raw')||jsonb_build_object('externalId',external_id,'creator',COALESCE(NULLIF(payload->>'creator',''),NULLIF(payload->'raw'->'creator'->>'name',''),'待归属')) AS row
   FROM bugs ORDER BY payload->>'createdAt' DESC,external_id LIMIT $5 OFFSET $6
 ) p),'[]'),
 'commits',COALESCE((SELECT jsonb_agg(p.row ORDER BY p.created DESC,p.id) FROM (
   SELECT external_id AS id,payload->>'authoredAt' AS created,(payload-'raw')||jsonb_build_object('externalId',external_id) AS row
   FROM commits ORDER BY payload->>'authoredAt' DESC,external_id LIMIT $5 OFFSET $7
 ) p),'[]')
)

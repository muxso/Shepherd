-- Calendar-day windows use Shanghai time, not sync observation timestamps.
WITH bounds AS (
  SELECT (now() AT TIME ZONE 'Asia/Shanghai')::date AS today, $2::int AS days
), facts AS (
  SELECT kind, payload,
    COALESCE(NULLIF(payload->>'creator',''), NULLIF(payload#>>'{raw,creator,name}',''),
      NULLIF(payload#>>'{raw,creator,displayName}',''), '待归属') AS creator,
    NULLIF(payload->>'createdAt','')::timestamptz AS created_at
  FROM ms_devops_entity WHERE source_id=$1 AND active
), events AS (
  SELECT CASE kind WHEN 'requirement' THEN 'requirements' WHEN 'bug' THEN 'bugs' ELSE 'testcases' END AS metric,
    creator AS actor, created_at AS happened_at, 1::bigint AS amount
  FROM facts WHERE kind IN ('requirement','bug','testcase')
  UNION ALL
  SELECT 'delivered', COALESCE(NULLIF(payload->>'owner',''),'待归属'),
    NULLIF(payload->>'statusChangedAt','')::timestamptz, 1
  FROM facts WHERE kind='requirement' AND payload->>'status'='已发布'
  UNION ALL
  SELECT m.metric, COALESCE(NULLIF(payload->>'author',''),'待归属'),
    NULLIF(payload->>'authoredAt','')::timestamptz, m.amount
  FROM facts CROSS JOIN LATERAL (VALUES
    ('commits',1::bigint),
    ('linkedCommits',CASE WHEN jsonb_array_length(COALESCE(payload->'requirementIds','[]'))>0 THEN 1 ELSE 0 END::bigint),
    ('additions',CASE WHEN jsonb_array_length(COALESCE(payload->'requirementIds','[]'))>0 THEN (payload->>'additions')::bigint END),
    ('deletions',CASE WHEN jsonb_array_length(COALESCE(payload->'requirementIds','[]'))>0 THEN (payload->>'deletions')::bigint END),
    ('unknownStats',CASE WHEN jsonb_array_length(COALESCE(payload->'requirementIds','[]'))>0 AND (payload->>'additions' IS NULL OR payload->>'deletions' IS NULL) THEN 1 ELSE 0 END::bigint)
  ) m(metric,amount) WHERE kind='commit'
), dated AS (
  SELECT *, (happened_at AT TIME ZONE 'Asia/Shanghai')::date AS day FROM events
), daily AS MATERIALIZED (
  SELECT metric, day, sum(amount) AS amount FROM dated GROUP BY metric, day
), totals AS (
  SELECT metric,
    sum(amount) FILTER (WHERE day BETWEEN today-days+1 AND today) AS current,
    sum(amount) FILTER (WHERE day BETWEEN today-days*2+1 AND today-days) AS previous,
    count(*) FILTER (WHERE day IS NULL AND amount>0) AS missing_time,
    sum(amount) FILTER (WHERE actor='待归属' AND day BETWEEN today-days+1 AND today) AS unattributed
  FROM dated CROSS JOIN bounds GROUP BY metric
), contributors AS (
  SELECT metric, actor AS name, sum(amount) AS count FROM dated CROSS JOIN bounds
  WHERE day BETWEEN today-days+1 AND today AND amount>0 GROUP BY metric, actor
), metrics AS (
  SELECT metric FROM (VALUES ('requirements'),('bugs'),('testcases'),('delivered'),
    ('commits'),('linkedCommits'),('additions'),('deletions'),('unknownStats')) m(metric)
)
SELECT jsonb_build_object(
  'from', today-days+1, 'through', today, 'timezone','Asia/Shanghai',
  'metrics', (SELECT jsonb_object_agg(m.metric,jsonb_build_object(
    'current',COALESCE(t.current,0), 'previous',COALESCE(t.previous,0),
    'missingTime',COALESCE(t.missing_time,0), 'unattributed',COALESCE(t.unattributed,0),
    'trend',(SELECT jsonb_agg(jsonb_build_object('date',d::date,'value',COALESCE((SELECT amount FROM daily e WHERE e.metric=m.metric AND day=d::date),0)) ORDER BY d)
      FROM generate_series((today-days+1)::timestamp,today::timestamp,'1 day') d),
    'contributors',COALESCE((SELECT jsonb_agg(jsonb_build_object('name',p.name,'count',p.count) ORDER BY p.count DESC,p.name)
      FROM contributors p WHERE p.metric=m.metric),'[]'::jsonb)
  )) FROM metrics m LEFT JOIN totals t USING(metric))
) FROM bounds

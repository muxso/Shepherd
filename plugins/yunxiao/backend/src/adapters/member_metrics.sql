-- One statement sees the source, clock and active facts in the same MVCC snapshot.
-- Only projection fields required for metrics leave storage; never raw descriptions.
WITH sonar_inputs AS (
 SELECT source_id,scope,task_id,repository_id,revision,analyzed_at,report
 FROM ms_devops_sonar_analysis WHERE source_id=$1 AND $3::jsonb IS NULL
 UNION ALL
 SELECT $1,report->>'scope',report->>'taskId',report->>'repositoryId',lower(report->>'revision'),
        (report->>'analyzedAt')::timestamptz,report
 FROM jsonb_array_elements(COALESCE($3,'[]'::jsonb)) AS entries(report)
)
SELECT jsonb_build_object('source',to_jsonb(s),'now',now(), 'records',COALESCE((
 SELECT jsonb_agg(jsonb_build_object('kind',f.kind,'externalId',f.external_id,'data',f.data)) FROM (
  SELECT kind,external_id,payload AS data
  FROM ms_devops_statistics_resolved WHERE source_id=s.id AND active AND kind IN ('requirement','bug')
  UNION ALL
  SELECT DISTINCT 'commit',COALESCE(payload->>'author',''),jsonb_build_object('author',payload->>'author','group',payload->>'group')
  FROM ms_devops_statistics_resolved WHERE source_id=s.id AND active AND kind='commit'
  UNION ALL
  SELECT 'sonar_push', analysis.task_id, analysis.report || jsonb_build_object('member', author.name, 'group', COALESCE(roster.group_name, identity.group_name))
  FROM sonar_inputs analysis
  LEFT JOIN ms_devops_author_alias alias ON alias.source_id=analysis.source_id
    AND alias.alias=btrim(analysis.report#>>'{push,author}')
  CROSS JOIN LATERAL (SELECT COALESCE(alias.name, NULLIF(btrim(analysis.report#>>'{push,author}'), '')) AS name) author
  LEFT JOIN ms_devops_author_identity identity ON identity.source_id=analysis.source_id AND identity.name=author.name
  LEFT JOIN ms_devops_activity_member roster ON roster.source_id=analysis.source_id AND roster.name=author.name
  WHERE analysis.source_id = s.id AND analysis.scope = 'sonar_repository'
    AND analysis.report ? 'push' AND author.name IS NOT NULL
    AND (analysis.report#>>'{push,pushedAt}')::timestamptz >= bounds.start_at
    AND (analysis.report#>>'{push,pushedAt}')::timestamptz < bounds.next_at
 ) f
),'[]')) FROM ms_devops_source s
CROSS JOIN LATERAL (
    SELECT to_date(COALESCE($2::text, to_char(now() AT TIME ZONE 'Asia/Shanghai','YYYY-MM')), 'YYYY-MM') AS month
) calendar
CROSS JOIN LATERAL (
    SELECT calendar.month::timestamp AT TIME ZONE 'Asia/Shanghai' AS start_at,
           (calendar.month + interval '1 month') AT TIME ZONE 'Asia/Shanghai' AS next_at
) bounds
WHERE s.id=$1

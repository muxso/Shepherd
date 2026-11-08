WITH resolved_code AS MATERIALIZED (
 SELECT kind,external_id,payload FROM ms_devops_statistics_resolved
 WHERE source_id=$1 AND active AND kind='commit'
), code_times AS MATERIALIZED (
 SELECT kind,external_id,payload,
   CASE WHEN payload->>'authoredAt' ~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}[Tt]' AND
     pg_input_is_valid(payload->>'authoredAt','timestamptz')
   THEN (payload->>'authoredAt')::timestamptz END AS authored_at
 FROM resolved_code
), activity_code AS (
 SELECT kind,external_id,payload FROM (
   SELECT *,row_number() OVER (
     PARTITION BY payload->>'author',payload->>'group'
     ORDER BY CASE WHEN authored_at<=now() THEN authored_at END DESC NULLS LAST,external_id
   ) AS latest
   FROM code_times
 ) ranked
 WHERE $2='code' AND (authored_at >= (((now() AT TIME ZONE 'Asia/Shanghai')::date - ($6::int-1))::timestamp AT TIME ZONE 'Asia/Shanghai')
   OR authored_at IS NULL OR latest=1)
), activity_facts AS MATERIALIZED (
 SELECT * FROM activity_code
 UNION ALL
 SELECT e.kind,e.external_id,e.payload FROM ms_devops_statistics_entity e WHERE e.source_id=$1 AND e.active AND (
     (e.kind='requirement' AND $3::bool) OR
     -- Keep a timezone margin here; the domain applies exact event bounds.
     (e.kind='bug' AND $4::bool AND $2 IN ('code','test') AND
       ($2='test' OR left(e.payload->>'createdAt',10) >= (((now() AT TIME ZONE 'Asia/Shanghai')::date - ($6::int+2))::text))) OR
     (e.kind='testcase' AND $5::bool AND $2='test'))
)
SELECT jsonb_build_object(
 'source',to_jsonb(s),'now',now(),
 'authorGroups',COALESCE((SELECT jsonb_agg(jsonb_build_object('author',author,'group',group_name)) FROM (SELECT DISTINCT payload->>'author' AS author,payload->>'group' AS group_name FROM resolved_code WHERE NULLIF(payload->>'author','') IS NOT NULL AND NULLIF(payload->>'group','') IS NOT NULL) g),'[]'),
 'reviewPolicy',COALESCE((SELECT policy FROM ms_devops_activity_review_policy WHERE source_id=s.id),$7::jsonb),
 'collectedSince',(SELECT min(finished_at) FROM ms_devops_sync_run WHERE source_id=s.id AND status='SUCCESS'),
 'testcasesCollected',COALESCE((SELECT counts ? 'testcase' FROM ms_devops_sync_run WHERE source_id=s.id AND status='SUCCESS' ORDER BY finished_at DESC LIMIT 1),false),
 'members',COALESCE((SELECT jsonb_agg(jsonb_build_object('name',m.name,'group',m.group_name,'roles',m.roles,'enabled',m.enabled) ORDER BY m.name) FROM ms_devops_activity_member m WHERE m.source_id=s.id),'[]'),
 'identities',COALESCE((SELECT jsonb_agg(jsonb_build_object('name',i.name,'group',i.group_name,'aliases',COALESCE((SELECT jsonb_agg(a.alias) FROM ms_devops_author_alias a WHERE a.source_id=i.source_id AND a.name=i.name AND a.alias<>i.name),'[]'))) FROM ms_devops_author_identity i WHERE i.source_id=s.id),'[]'),
 'records',COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',e.kind,'externalId',e.external_id,'data',
   jsonb_build_object('title',d."title",'serial',d."serial",
     'url',COALESCE(NULLIF(d."url",''),NULLIF(d.raw#>>'{url}',''),NULLIF(d.raw#>>'{webUrl}',''),NULLIF(d.raw#>>'{web_url}','')),
     'requirementIds',d."requirementIds",'parentId',d."parentId",'parentSerial',d."parentSerial",'branches',d."branches") ||
   CASE e.kind
   WHEN 'commit' THEN jsonb_build_object(
     'author',d."author",'authorRaw',d."authorRaw",'group',d."group",
     'repoId',d."repoId",'repo',d."repo",'sha',d."sha",
     'authoredAt',d."authoredAt",'additions',d."additions",'deletions',d."deletions",
     'isMerge',d."isMerge",'parentIds',d."parentIds",
     'repositoryUrl',d."repositoryUrl",'repoUrl',d."repoUrl")
   WHEN 'requirement' THEN jsonb_build_object(
     'creator',COALESCE(NULLIF(d."creator",''),NULLIF(d.raw#>>'{creator,name}',''),NULLIF(d.raw#>>'{creator,displayName}',''),NULLIF(d."creatorInfo"->>'name','')),
     'owner',d."owner",'group',d."group",'status',d."status",
     'createdAt',d."createdAt",'statusChangedAt',d."statusChangedAt")
   WHEN 'bug' THEN jsonb_build_object(
     'creator',COALESCE(NULLIF(d."creator",''),NULLIF(d.raw#>>'{creator,name}',''),NULLIF(d.raw#>>'{creator,displayName}',''),NULLIF(d."creatorInfo"->>'name','')),
     'status',d."status",'createdAt',d."createdAt",
     'customFieldValues',COALESCE((SELECT jsonb_agg(field) FROM jsonb_array_elements(
       CASE WHEN jsonb_typeof(d."customFieldValues")='array' THEN d."customFieldValues"
         WHEN jsonb_typeof(d.raw#>'{customFieldValues}')='array' THEN d.raw#>'{customFieldValues}'
         ELSE '[]'::jsonb END) field WHERE field->>'fieldName'='缺陷责任人'),'[]'))
   WHEN 'testcase' THEN jsonb_build_object(
     'creator',COALESCE(NULLIF(d."creator",''),NULLIF(d.raw#>>'{creator,name}',''),NULLIF(d.raw#>>'{creator,displayName}',''),NULLIF(d."creatorInfo"->>'name','')),
     'createdAt',d."createdAt",'status',d."status",'repoId',d."repoId",'repo',d."repo")
   ELSE '{}'::jsonb END))
   FROM activity_facts e CROSS JOIN LATERAL jsonb_to_record(e.payload) AS d("title" jsonb, "serial" jsonb, "url" text, "requirementIds" jsonb, "parentId" jsonb, "parentSerial" jsonb, "branches" jsonb, "author" jsonb, "authorRaw" jsonb, "group" jsonb, "repoId" jsonb, "repo" jsonb, "sha" jsonb, "authoredAt" jsonb, "additions" jsonb, "deletions" jsonb, "isMerge" jsonb, "parentIds" jsonb, "repositoryUrl" text, "repoUrl" text, "creator" text, "owner" text, "status" jsonb, "createdAt" text, "statusChangedAt" text, "customFieldValues" jsonb, "creatorInfo" jsonb, raw jsonb) ),'[]')
) FROM ms_devops_source s WHERE s.id=$1

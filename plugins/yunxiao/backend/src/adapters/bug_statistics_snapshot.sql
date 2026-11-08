WITH facts AS MATERIALIZED (
 SELECT kind,external_id,payload FROM ms_devops_entity WHERE source_id=$1 AND active AND kind IN ('bug','requirement')
), rows AS (
 SELECT kind,external_id,CASE kind WHEN 'requirement' THEN jsonb_build_object('serial',payload->'serial','owner',payload->'owner','group',payload->'group')
 ELSE jsonb_build_object('creator',COALESCE(NULLIF(payload->>'creator',''),payload#>>'{raw,creator,name}',payload#>>'{raw,creator,displayName}'),'owner',payload->'owner','group',payload->'group','createdAt',payload->'createdAt','status',payload->'status','priority',payload->'priority','parentId',payload->'parentId','parentSerial',payload->'parentSerial','requirementIds',payload->'requirementIds','customFieldValues',COALESCE((SELECT jsonb_agg(f) FROM jsonb_array_elements(CASE WHEN jsonb_typeof(payload->'customFieldValues')='array' THEN payload->'customFieldValues' WHEN jsonb_typeof(payload#>'{raw,customFieldValues}')='array' THEN payload#>'{raw,customFieldValues}' ELSE '[]'::jsonb END) f WHERE f->>'fieldName'='缺陷责任人'),'[]'::jsonb)) END AS data
 FROM facts
 UNION ALL
 SELECT DISTINCT 'commit',payload->>'author',jsonb_build_object('author',payload->'author','group',payload->'group')
 FROM ms_devops_resolved_entity WHERE source_id=$1 AND active AND kind='commit' AND NULLIF(payload->>'author','') IS NOT NULL
)
SELECT jsonb_build_object('source',to_jsonb(s),'now',now(),'records',COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',kind,'externalId',external_id,'data',data)) FROM rows),'[]'::jsonb)) FROM ms_devops_source s WHERE s.id=$1

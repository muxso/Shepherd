-- One statement sees the source, clock and active facts in the same MVCC snapshot.
-- Only projection fields required for metrics leave storage; never raw descriptions.
SELECT jsonb_build_object('source',to_jsonb(s),'now',now(), 'records',COALESCE((
 SELECT jsonb_agg(jsonb_build_object('kind',f.kind,'externalId',f.external_id,'data',f.data)) FROM (
  SELECT kind,external_id,(payload-'raw') || jsonb_build_object('customFieldValues',COALESCE((
    SELECT jsonb_agg(field) FROM jsonb_array_elements(CASE WHEN jsonb_typeof(payload#>'{raw,customFieldValues}')='array' THEN payload#>'{raw,customFieldValues}' ELSE '[]'::jsonb END) field
    WHERE field->>'fieldName' IN ('开发者','测试人员','协同人','协作人','需求提出人')
  ),'[]')) AS data
  FROM ms_devops_entity WHERE source_id=s.id AND active AND kind IN ('requirement','bug')
  UNION ALL
  SELECT DISTINCT 'commit',COALESCE(payload->>'author',''),jsonb_build_object('author',payload->>'author','group',payload->>'group')
  FROM ms_devops_entity WHERE source_id=s.id AND active AND kind='commit'
 ) f
),'[]')) FROM ms_devops_source s WHERE s.id=$1

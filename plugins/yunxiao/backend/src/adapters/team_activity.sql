SELECT jsonb_build_object(
 'source',to_jsonb(s),'now',now(),
 'collectedSince',(SELECT min(finished_at) FROM ms_devops_sync_run WHERE source_id=s.id AND status='SUCCESS'),
 'testcasesCollected',COALESCE((SELECT counts ? 'testcase' FROM ms_devops_sync_run WHERE source_id=s.id AND status='SUCCESS' ORDER BY finished_at DESC LIMIT 1),false),
 'members',COALESCE((SELECT jsonb_agg(jsonb_build_object('name',m.name,'group',m.group_name,'roles',m.roles,'enabled',m.enabled) ORDER BY m.name) FROM ms_devops_activity_member m WHERE m.source_id=s.id),'[]'),
 'identities',COALESCE((SELECT jsonb_agg(jsonb_build_object('name',i.name,'group',i.group_name,'aliases',COALESCE((SELECT jsonb_agg(a.alias) FROM ms_devops_author_alias a WHERE a.source_id=i.source_id AND a.name=i.name AND a.alias<>i.name),'[]'))) FROM ms_devops_author_identity i WHERE i.source_id=s.id),'[]'),
 'records',COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',e.kind,'externalId',e.external_id,'data',
   (e.payload-'raw'-'reopenHistory') || jsonb_build_object(
     'url',COALESCE(NULLIF(e.payload->>'url',''),NULLIF(e.payload#>>'{raw,url}',''),NULLIF(e.payload#>>'{raw,webUrl}',''),NULLIF(e.payload#>>'{raw,web_url}','')),
     'creator',COALESCE(NULLIF(e.payload->>'creator',''),NULLIF(e.payload#>>'{raw,creator,name}',''),NULLIF(e.payload#>>'{raw,creator,displayName}','')),
     'customFieldValues',COALESCE((SELECT jsonb_agg(field) FROM jsonb_array_elements(CASE WHEN jsonb_typeof(e.payload#>'{raw,customFieldValues}')='array' THEN e.payload#>'{raw,customFieldValues}' ELSE '[]'::jsonb END) field WHERE field->>'fieldName'='缺陷责任人'),'[]'))))
   FROM ms_devops_resolved_entity e WHERE e.source_id=s.id AND e.active AND e.kind IN ('requirement','bug','commit','testcase')),'[]')
) FROM ms_devops_source s WHERE s.id=$1

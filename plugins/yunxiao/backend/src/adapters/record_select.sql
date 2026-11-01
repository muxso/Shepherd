SELECT (e.payload-'raw') || jsonb_build_object('externalId',e.external_id,'creator',COALESCE(NULLIF(e.payload->>'creator',''),NULLIF(e.payload->'raw'->'creator'->>'name',''),'待归属'),
'requirements',COALESCE((SELECT jsonb_agg(to_jsonb(resolved) ORDER BY resolved."externalId") FROM (
 SELECT DISTINCT COALESCE(r.external_id,link.id) AS "externalId",r.payload->>'title' AS title,r.payload->>'serial' AS serial
 FROM jsonb_array_elements_text(CASE
 WHEN e.kind='commit' THEN COALESCE(e.payload->'requirementIds','[]')
 WHEN e.kind='bug' THEN COALESCE(e.payload->'requirementIds','[]')||to_jsonb(array_remove(ARRAY[NULLIF(e.payload->>'parentId',''),NULLIF(e.payload->>'parentSerial','')],NULL))
 ELSE '[]'::jsonb END) link(id)
 LEFT JOIN LATERAL (SELECT r.external_id,r.payload FROM ms_devops_entity r
 WHERE r.source_id=e.source_id AND r.kind='requirement' AND r.active AND (r.external_id=link.id OR r.payload->>'serial'=link.id)
 ORDER BY (r.external_id=link.id) DESC,r.external_id LIMIT 1) r ON true
 WHERE link.id<>'' AND (e.kind='commit' OR r.external_id IS NOT NULL)
) resolved),'[]'::jsonb))
FROM ms_devops_entity e WHERE {filters} ORDER BY {order} LIMIT {limit} OFFSET {offset}

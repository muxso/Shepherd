-- Resolve aliases with indexed joins and aggregate fallback groups once.
CREATE OR REPLACE VIEW ms_devops_resolved_entity AS
SELECT e.source_id,e.kind,e.external_id,
 CASE WHEN i.name IS NULL THEN e.payload ELSE e.payload || jsonb_build_object(
   'author',i.name,
   'authorRaw',COALESCE(NULLIF(e.payload->>'authorRaw',''),e.payload->>'author'),
   'group',COALESCE(i.group_name,canonical_group.name,'待归属')) END AS payload,
 e.active,e.first_seen_at,e.observed_at
FROM ms_devops_entity e
LEFT JOIN ms_devops_author_alias raw_alias
 ON e.kind='commit' AND raw_alias.source_id=e.source_id
 AND raw_alias.alias=btrim(e.payload->>'authorRaw')
LEFT JOIN ms_devops_author_alias normalized_alias
 ON e.kind='commit' AND raw_alias.name IS NULL AND normalized_alias.source_id=e.source_id
 AND normalized_alias.alias=btrim(e.payload->>'author')
LEFT JOIN ms_devops_author_identity i
 ON i.source_id=e.source_id AND i.name=COALESCE(raw_alias.name,normalized_alias.name)
LEFT JOIN (
 SELECT source_id,payload->>'author' AS author,
   CASE WHEN count(DISTINCT payload->>'group')=1 THEN min(payload->>'group') END AS name
 FROM ms_devops_entity
 WHERE active AND kind='commit' AND NULLIF(btrim(payload->>'group'),'') IS NOT NULL
   AND payload->>'group'<>'待归属'
 GROUP BY source_id,payload->>'author'
) canonical_group
 ON i.group_name IS NULL AND canonical_group.source_id=e.source_id AND canonical_group.author=i.name;

INSERT INTO ms_requirement_version(requirement_id,version,description,acceptance_criteria,created_at)
SELECT r.id,r.baseline_version,COALESCE(e.payload->'raw'->>'description',''),'{}'::text[],(extract(epoch FROM r.updated_at)*1000)::bigint
FROM ms_requirement r JOIN ms_devops_entity e ON e.source_id=r.external_source_id AND e.external_id=r.external_id AND e.kind='requirement' AND e.active
WHERE r.external_source_id=$1
ON CONFLICT(requirement_id,version) DO NOTHING

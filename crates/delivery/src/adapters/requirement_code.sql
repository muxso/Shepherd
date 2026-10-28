SELECT c.payload->>'repoId' AS repo_id,COALESCE(c.payload->>'repo','') AS repo,
 COALESCE(c.payload->>'sha',c.external_id) AS sha,COALESCE(c.payload->>'title','') AS title,
 COALESCE(c.payload->>'author','') AS author,COALESCE(c.payload->>'authoredAt','') AS authored_at,
 (c.payload->>'additions')::bigint AS additions,(c.payload->>'deletions')::bigint AS deletions,
 COALESCE(c.payload->>'url','') AS url
FROM ms_requirement r JOIN ms_devops_entity c ON c.source_id=r.external_source_id AND c.kind='commit' AND c.active
LEFT JOIN ms_devops_entity source_req ON source_req.source_id=r.external_source_id AND source_req.kind='requirement' AND source_req.external_id=r.external_id AND source_req.active
WHERE r.id=$2 AND r.project_id=$1 AND NOT r.deleted
 AND (c.payload->'requirementIds' ? r.external_id OR c.payload->'requirementIds' ? (source_req.payload->>'serial'))
 AND COALESCE(c.payload->>'isMerge','false')='false'
 AND jsonb_array_length(CASE WHEN jsonb_typeof(c.payload->'parentIds')='array' THEN c.payload->'parentIds' ELSE '[]' END)<=1
ORDER BY c.payload->>'authoredAt' DESC,c.external_id

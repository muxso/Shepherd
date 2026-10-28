-- $1 project, $2 optional requirement. Tasks always take precedence, even when
-- none has passed acceptance yet. A fallback has no estimated points or AI credit.
SELECT t.points, t.verified_at, dc.requirement_id,
 (SELECT count(*) FROM ms_delivery_attempt a
  WHERE a.decomposition_id=t.decomposition_id AND a.task_id=t.id) AS attempt_cnt,
 EXISTS(SELECT 1 FROM ms_delivery_attempt a
        WHERE a.decomposition_id=t.decomposition_id AND a.task_id=t.id
          AND a.status='DELIVERED') AS ai,
 false AS requirement_unit
FROM ms_task t
JOIN ms_task_decomposition dc ON dc.id=t.decomposition_id
JOIN ms_requirement r ON r.id=dc.requirement_id
WHERE t.status='VERIFIED' AND NOT r.deleted AND r.project_id=$1 AND ($2='' OR r.id=$2)
UNION ALL
SELECT 0 AS points,
 COALESCE((SELECT max(s.finished_at) FROM ms_requirement_stage s
           WHERE s.requirement_id=r.id AND s.stage='DELIVERY' AND s.status='DONE'),
          CASE WHEN e.payload->'raw'->>'updateStatusAt' ~ '^[0-9]{13}$'
               THEN to_timestamp((e.payload->'raw'->>'updateStatusAt')::double precision/1000) END) AS verified_at,
 r.id AS requirement_id, 0::bigint AS attempt_cnt, false AS ai, true AS requirement_unit
FROM ms_requirement r
LEFT JOIN ms_devops_entity e ON e.source_id=r.external_source_id
 AND e.external_id=r.external_id AND e.kind='requirement' AND e.active
WHERE r.status='DELIVERED' AND NOT r.deleted AND r.project_id=$1 AND ($2='' OR r.id=$2)
 AND NOT EXISTS (SELECT 1 FROM ms_task_decomposition dc
                 JOIN ms_task t ON t.decomposition_id=dc.id WHERE dc.requirement_id=r.id)

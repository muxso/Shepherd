WITH entity AS (
    SELECT s.name, s.created_by,
        (SELECT ch.user_id FROM ms_api_scenario_change ch
         WHERE ch.scenario_id = s.id AND ch.action <> 'CREATE' AND ch.user_id IS NOT NULL
         ORDER BY ch.created_at DESC, ch.id DESC LIMIT 1) AS changer
    FROM ms_api_scenario s WHERE $4 = 'SCENARIO' AND s.id = $2 AND s.project_id = $1
    UNION ALL
    SELECT p.name, p.created_by, NULL::text
    FROM ms_test_plan p WHERE $4 IN ('PLAN', 'TEST_PLAN') AND p.id = $2 AND p.project_id = $1
)
SELECT COALESCE(p.name, ''), COALESCE(entity.name, ''),
       COALESCE(trigger_user.name, NULLIF($3, ''), ''),
       COALESCE(change_user.name, ''), COALESCE(creator_user.name, ''),
       COALESCE((
           SELECT string_agg(f.name, '、' ORDER BY f.name)
           FROM (
               SELECT DISTINCT u.name
               FROM ms_follow rel JOIN ms_user u ON u.id = rel.user_id
               WHERE rel.project_id = $1 AND rel.entity_id = $2
                 AND (lower(rel.entity_type) = lower($4)
                   OR ($4 = 'SCENARIO' AND lower(rel.entity_type) = 'api_scenario')
                   OR ($4 IN ('PLAN','TEST_PLAN') AND lower(rel.entity_type) IN ('plan','test_plan')))
               ORDER BY u.name LIMIT 20
           ) f
       ), '')
FROM ms_project p
LEFT JOIN entity ON TRUE
LEFT JOIN ms_user trigger_user ON trigger_user.id = $3
LEFT JOIN ms_user change_user ON change_user.id = entity.changer
LEFT JOIN ms_user creator_user ON creator_user.id = entity.created_by
WHERE p.id = $1 AND p.deleted = FALSE

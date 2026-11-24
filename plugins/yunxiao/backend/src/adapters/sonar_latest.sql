SELECT jsonb_build_object('items', COALESCE((
    SELECT jsonb_agg(report ORDER BY analyzed_at DESC, task_id DESC)
    FROM (
        SELECT * FROM (
            SELECT DISTINCT ON (scope, repository_id, pull_request) report, analyzed_at, task_id
            FROM ms_devops_sonar_analysis WHERE source_id = $1
            ORDER BY scope, repository_id, pull_request, analyzed_at DESC, task_id DESC
        ) AS latest
        ORDER BY analyzed_at DESC, task_id DESC
        LIMIT 100
    ) AS recent
), '[]'::jsonb))
FROM ms_devops_source WHERE id = $1;

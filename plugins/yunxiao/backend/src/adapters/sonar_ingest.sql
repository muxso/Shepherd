WITH inserted AS (
    INSERT INTO ms_devops_sonar_analysis (
        source_id, task_id, repository_id, pull_request, revision, analyzed_at, report, scope
    )
    SELECT id, $2, $3, $4, $5, $6, $7, $8
    FROM ms_devops_source
    WHERE id = $1
    ON CONFLICT (source_id, scope, task_id) DO NOTHING
    RETURNING task_id
)
SELECT EXISTS (SELECT FROM inserted)
FROM ms_devops_source WHERE id = $1;

WITH actors AS (
    SELECT min(id) AS id, name
    FROM ms_user
    WHERE NULLIF(trim(name), '') IS NOT NULL
    GROUP BY name HAVING count(*) = 1
)
SELECT actors.name AS member,
       to_char(report.finished_at AT TIME ZONE 'Asia/Shanghai', 'YYYY-MM-DD') AS date,
       count(*) FILTER (WHERE result.outcome = 'SUCCESS') AS passed,
       count(*) AS evaluated
FROM ms_api_batch_report report
JOIN actors ON actors.id = report.scenario_triggered_by
JOIN ms_api_case_result result ON result.report_id = report.id
JOIN LATERAL (
    SELECT e.project_id
    FROM ms_api_scenario_execution e
    WHERE e.report_id = report.id AND e.status IN ('SUCCESS', 'ERROR')
    LIMIT 1
) execution ON TRUE
JOIN ms_project project ON project.id = execution.project_id AND NOT project.deleted
WHERE report.finished_at >= (((now() AT TIME ZONE 'Asia/Shanghai')::date - ($2::int - 1))::timestamp AT TIME ZONE 'Asia/Shanghai')
  AND report.finished_at <= now()
  AND report.status IN ('SUCCESS', 'ERROR') AND result.outcome IN ('SUCCESS', 'ERROR')
  AND ($3::bool OR EXISTS (
      SELECT 1 FROM ms_project_member member WHERE member.project_id = project.id AND member.user_id = $1
  ))
GROUP BY actors.name, 2
ORDER BY 2, actors.name

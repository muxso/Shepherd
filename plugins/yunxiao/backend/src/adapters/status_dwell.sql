WITH scope AS (
    SELECT last_success_at AS through,
           date_trunc('month', last_success_at AT TIME ZONE 'Asia/Shanghai') AS sync_month
    FROM ms_devops_source
    WHERE id = $1
), bounds AS (
    SELECT coalesce($5::timestamptz, sync_month AT TIME ZONE 'Asia/Shanghai') AS month_start,
           least(through, coalesce($6::timestamptz,
                 (sync_month + interval '1 month') AT TIME ZONE 'Asia/Shanghai')) AS month_end
    FROM scope
    WHERE through IS NOT NULL
), changes AS (
    SELECT external_id, old_status, new_status, observed_at, id,
           CASE WHEN pg_input_is_valid(source_changed_at, 'timestamptz')
                THEN source_changed_at::timestamptz END AS changed_at
    FROM ms_devops_history
    WHERE source_id = $1 AND kind = $2
), checked AS (
    SELECT *,
           lag(changed_at) OVER (PARTITION BY external_id ORDER BY observed_at, id) AS previous_changed_at
    FROM changes
), event_times AS (
    SELECT *,
           CASE WHEN bool_and(coalesce(isfinite(changed_at) AND changed_at <= observed_at
                       AND (previous_changed_at IS NULL OR previous_changed_at < changed_at), false))
                       OVER (PARTITION BY external_id)
                THEN changed_at ELSE observed_at END AS entered_at
    FROM checked
), timeline AS (
    SELECT *,
           lead(old_status) OVER sequence AS next_old_status,
           lead(entered_at) OVER sequence AS ended_at,
           row_number() OVER (
               PARTITION BY external_id ORDER BY observed_at DESC, id DESC
           ) AS latest
    FROM event_times
    WINDOW sequence AS (PARTITION BY external_id ORDER BY observed_at, id)
), completed AS (
    SELECT external_id, new_status AS status, 'completed' AS phase, entered_at, ended_at
    FROM timeline
    WHERE new_status = next_old_status
), entities AS MATERIALIZED (
    SELECT external_id, active, payload->>'status' AS status,
           payload->>'statusChangedAt' AS status_changed_at,
           payload->>'serial' AS serial, payload->>'title' AS title
    FROM ms_devops_statistics_entity
    WHERE source_id = $1 AND kind = $2
), current_status AS MATERIALIZED (
    SELECT e.external_id, e.status,
           CASE WHEN pg_input_is_valid(e.status_changed_at, 'timestamptz')
                THEN e.status_changed_at::timestamptz END AS changed_at,
           t.entered_at AS latest_entry
    FROM entities e
    LEFT JOIN timeline t ON t.external_id = e.external_id AND t.latest = 1
                         AND t.new_status = e.status
    WHERE e.active
), intervals AS (
    SELECT * FROM completed
    UNION ALL
    SELECT external_id, status, 'current',
           CASE WHEN isfinite(changed_at) AND changed_at <= through
                THEN greatest(changed_at, latest_entry) ELSE latest_entry END,
           through
    FROM current_status CROSS JOIN scope
), monthly_stages AS (
    SELECT external_id, status, phase, entered_at, ended_at,
           extract(epoch FROM least(ended_at, month_end) - greatest(entered_at, month_start)) AS seconds
    FROM intervals CROSS JOIN bounds
    WHERE $4 AND greatest(entered_at, month_start) < least(ended_at, month_end)
        AND entered_at < ended_at AND nullif(btrim(status), '') IS NOT NULL
), stage_totals AS (
    SELECT external_id, sum(seconds)::float8 AS seconds,
           sum(seconds) FILTER (WHERE status = $10) AS stage_seconds, count(*) AS stage_count
    FROM monthly_stages
    GROUP BY external_id
), classified AS (
    SELECT *, CASE WHEN $2 = 'requirement' AND status = '就绪（待开发）' THEN '待开发'
                   ELSE status END AS filter_status
    FROM entities
), monthly_work AS (
    SELECT e.external_id, e.serial, e.title, e.status AS current_status,
           s.seconds, s.stage_seconds, coalesce(s.stage_count, 0) AS stage_count
    FROM classified e
    LEFT JOIN stage_totals s USING (external_id)
    WHERE $4 AND (
        ($12 = '' AND s.external_id IS NOT NULL)
        OR ($12 <> '' AND e.active AND e.filter_status = $12)
    ) AND ($7 = '' OR strpos(lower(e.external_id), lower($7)) > 0
        OR strpos(lower(e.serial), lower($7)) > 0
        OR strpos(lower(e.title), lower($7)) > 0)
), current_counts AS (
    SELECT filter_status, count(*) AS count
    FROM classified
    WHERE $4 AND active AND nullif(btrim(filter_status), '') IS NOT NULL
    GROUP BY filter_status
), ranked AS (
    SELECT *, row_number() OVER (ORDER BY
        CASE WHEN $11 THEN stage_seconds END ASC NULLS LAST,
        CASE WHEN NOT $11 THEN stage_seconds END DESC NULLS LAST,
        stage_count DESC, seconds DESC, external_id
    ) AS position
    FROM monthly_work
), page AS (
    SELECT * FROM ranked ORDER BY position LIMIT $8 OFFSET $9
), page_stages AS (
    SELECT s.external_id, jsonb_agg(jsonb_build_object(
               'status', s.status, 'phase', s.phase, 'enteredAt', s.entered_at,
               'exitedAt', CASE WHEN s.phase = 'completed' THEN s.ended_at END,
               'seconds', s.seconds
           ) ORDER BY s.entered_at, s.ended_at, s.status) AS stages
    FROM monthly_stages s JOIN page USING (external_id)
    GROUP BY s.external_id
), samples AS (
    SELECT status, phase, extract(epoch FROM ended_at - entered_at) AS seconds
    FROM intervals CROSS JOIN scope
    WHERE NOT $4 AND nullif(btrim(status), '') IS NOT NULL
      AND ended_at <= through AND (
        (phase = 'current' AND entered_at <= ended_at)
        OR (phase = 'completed' AND entered_at < ended_at AND ended_at > through - make_interval(days => $3))
      )
), statistics AS (
    SELECT status, phase, count(*) AS count, avg(seconds)::float8 AS "averageSeconds"
    FROM samples
    GROUP BY status, phase
), summary AS (
    SELECT coalesce(jsonb_agg(to_jsonb(statistics) ORDER BY "averageSeconds" DESC, status), '[]'::jsonb) AS rows
    FROM statistics
)
SELECT jsonb_build_object('asOf', (SELECT through FROM scope), 'items', rows)
       || CASE WHEN $4 THEN jsonb_build_object(
           'month', coalesce((SELECT to_char(month_start AT TIME ZONE 'Asia/Shanghai', 'YYYY-MM') FROM bounds),
                             to_char($5::timestamptz AT TIME ZONE 'Asia/Shanghai', 'YYYY-MM')),
           'total', (SELECT count(*) FROM monthly_work),
           'currentStatusCounts', (SELECT coalesce(jsonb_object_agg(filter_status, count), '{}'::jsonb)
                                   FROM current_counts),
           'stages', (SELECT coalesce(jsonb_agg(status ORDER BY status), '[]'::jsonb)
                      FROM (SELECT DISTINCT status FROM monthly_stages) names),
           'workItems', (SELECT coalesce(jsonb_agg(jsonb_build_object(
               'externalId', external_id, 'serial', serial, 'title', title, 'currentStatus', current_status,
               'seconds', seconds, 'stages', coalesce(stages, '[]'::jsonb)
           ) ORDER BY position), '[]'::jsonb) FROM page LEFT JOIN page_stages USING (external_id))
       ) ELSE '{}'::jsonb END
FROM summary

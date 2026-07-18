-- Per-project display number for API scenarios (100001…), shown as the list ID.

ALTER TABLE ms_api_scenario ADD COLUMN IF NOT EXISTS num bigint;

WITH numbered AS (
    SELECT id, 100000 + ROW_NUMBER() OVER (PARTITION BY project_id ORDER BY created_at, id) AS rn
    FROM ms_api_scenario WHERE num IS NULL
)
UPDATE ms_api_scenario s SET num = n.rn FROM numbered n WHERE s.id = n.id;

CREATE UNIQUE INDEX IF NOT EXISTS ux_api_scenario_num ON ms_api_scenario (project_id, num);
